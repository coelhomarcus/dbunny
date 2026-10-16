//! One connection to a SQLite file. rusqlite is blocking, so the connection sits behind
//! a mutex and every use runs on tokio's blocking threads.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::{Connection, InterruptHandle, OpenFlags, MAIN_DB};

use super::errors::{cannot_open, map_open_error, missing};
use super::OpenOptions;

pub(crate) struct SqliteConn {
    /// The connection; None once the session was closed. A task that already took the
    /// connection out keeps it working until the task ends.
    slot: Mutex<Option<Arc<Mutex<Connection>>>>,
    /// Stops the statement running on the connection, from any thread.
    interrupt: InterruptHandle,
    /// The connection can't write: opened read-only, or the file only allows reading.
    read_only: bool,
}

impl SqliteConn {
    /// Opens `path` without changing the file: no journal mode change, no extensions.
    /// Blocks; call it off the async threads.
    pub fn open(path: &Path, opts: &OpenOptions) -> Result<SqliteConn, String> {
        match std::fs::metadata(path) {
            Ok(meta) if meta.is_dir() => return Err(cannot_open(path, "it's a folder.")),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if !opts.create {
                    return Err(missing(path));
                }
            }
            Err(e) => return Err(cannot_open(path, &e.to_string())),
        }

        let mode = if opts.read_only {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        } else if opts.create {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        };
        // The bundled SQLite reads "file:..." names as URIs whatever the flags say, so the driver
        // hands over absolute paths only (see SqliteDriver::open)
        let flags = mode | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let fail = |e: rusqlite::Error| map_open_error(&e, path);

        let conn = Connection::open_with_flags(path, flags).map_err(fail)?;
        conn.busy_timeout(opts.busy_timeout).map_err(fail)?;
        // For this connection only: edits respect foreign keys like they do in PostgreSQL
        conn.execute_batch("PRAGMA foreign_keys = ON").map_err(fail)?;
        // Reading the schema is what tells a database from any other file
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0))
            .map_err(fail)?;

        let read_only = opts.read_only || conn.is_readonly(MAIN_DB).map_err(fail)?;
        let interrupt = conn.get_interrupt_handle();
        Ok(SqliteConn { slot: Mutex::new(Some(Arc::new(Mutex::new(conn)))), interrupt, read_only })
    }

    /// Runs `f` with the connection, on a blocking thread, once the connection is free.
    #[allow(dead_code)] // Used once the SQLite catalog and data land
    pub async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let conn = lock(&self.slot).clone().ok_or_else(|| "Session not found".to_string())?;
        tokio::task::spawn_blocking(move || f(&mut lock(&conn)))
            .await
            .map_err(|e| format!("The query stopped unexpectedly: {e}"))?
    }

    /// Stops what the connection is running; it fails with "canceling statement due to user request".
    #[allow(dead_code)] // Used once the query editor can cancel on SQLite
    pub fn interrupt(&self) {
        self.interrupt.interrupt();
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// Lets go of the connection; it closes once no running task holds it.
    pub fn close(&self) {
        let closed = lock(&self.slot).take();
        drop(closed);
    }
}

/// A mutex a panicking task left poisoned still guards a usable value.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
