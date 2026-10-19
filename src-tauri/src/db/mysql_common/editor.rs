//! The query editor's own connection. What's typed there (a transaction, SET, a long query)
//! stays on it instead of reaching the pool behind table views and the explorer.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use futures_util::future::BoxFuture;
use mysql_async::consts::StatusFlags;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Opts};

use crate::db::mysql_common::connect::MysqlEndpoint;
use crate::db::mysql_common::errors::error_message;

/// The error of the first command after a connection with an open transaction was lost.
pub(crate) const ROLLED_BACK: &str = "The connection was lost; the open transaction was rolled back";

/// How long a ping may take before the connection counts as lost.
const PING_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct EditorConn {
    /// The connection, None inside once it was lost (the next command opens another). The
    /// slot is None once the session was closed; a task that already took the connection
    /// out keeps it working until the task ends.
    slot: Mutex<Option<Arc<tokio::sync::Mutex<Option<Conn>>>>>,
    /// The server's id of the current connection, for `KILL QUERY` from another connection.
    id: AtomicU32,
    /// Whether a transaction is open, readable without waiting for the connection. `run` sets it
    /// after each command from the server's own status; a connection lost while it is set
    /// reports the rolled back transaction once, on the next command.
    in_transaction: AtomicBool,
    /// Set by a cancel, for a script to stop before its next statement. A script clears it
    /// when it starts, so a cancel that came with nothing running is forgotten.
    cancelled: Arc<AtomicBool>,
    /// The database the connection was in after the console's last script, when it isn't the
    /// session's own (a `USE`); a connection opened again goes back to it.
    database: Mutex<Option<String>>,
    /// How to open the connection again.
    opts: Mutex<Opts>,
    /// For the messages of connection errors.
    endpoint: MysqlEndpoint,
    #[cfg(test)]
    test_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
}

impl EditorConn {
    /// Opens the connection.
    pub(crate) async fn open(opts: Opts, endpoint: MysqlEndpoint) -> Result<EditorConn, String> {
        let conn = Conn::new(opts.clone()).await.map_err(|e| error_message(&e, &endpoint))?;
        Ok(EditorConn {
            id: AtomicU32::new(conn.id()),
            slot: Mutex::new(Some(Arc::new(tokio::sync::Mutex::new(Some(conn))))),
            in_transaction: AtomicBool::new(false),
            cancelled: Arc::default(),
            database: Mutex::new(None),
            opts: Mutex::new(opts),
            endpoint,
            #[cfg(test)]
            test_pause: Mutex::new(None),
        })
    }

    /// Runs `f` with the connection once it's free. A connection that was lost is replaced
    /// first: a ping tells, so the command doesn't fail on a connection that dropped while
    /// idle. If a transaction was open on it, that command fails with `ROLLED_BACK` instead
    /// of running, and the one after runs normally. When `f` fails, a ping tells whether the
    /// connection went with it, so the next command opens another. Each call pings once, so
    /// a script runs its statements inside one call.
    pub(crate) async fn run<T>(
        &self,
        f: impl for<'c> FnOnce(&'c mut Conn) -> BoxFuture<'c, Result<T, String>>,
    ) -> Result<T, String> {
        let shared = lock(&self.slot).clone().ok_or_else(session_not_found)?;
        let mut held = shared.lock().await;
        // Closed while this command waited for the one before it
        if lock(&self.slot).is_none() {
            return Err(session_not_found());
        }
        #[cfg(test)]
        self.pause_for_test().await;
        if let Some(conn) = held.as_mut() {
            if !alive(conn).await {
                *held = None;
            }
        }
        let mut reconnected = false;
        if held.is_none() {
            let opts = lock(&self.opts).clone();
            let conn = Conn::new(opts).await.map_err(|e| error_message(&e, &self.endpoint))?;
            self.id.store(conn.id(), Ordering::SeqCst);
            *held = Some(conn);
            reconnected = true;
        }
        // Checked again with the connection `f` will use already in `id`: `close()` takes the
        // slot first and reads `id` after, so either this sees the session closed (and drops
        // a connection `close()` may never have seen), or `close()` kills this very connection.
        // Without it, a connection opened while `close()` ran would run `f` after the close.
        if lock(&self.slot).is_none() {
            *held = None;
            return Err(session_not_found());
        }
        if reconnected {
            // A new connection starts in the session's database; the editor goes back to the
            // one a `USE` picked, or nothing runs, so no statement lands in the wrong database
            let picked = lock(&self.database).clone();
            if let Some(name) = picked {
                let conn = held.as_mut().expect("connected above");
                if let Err(e) = conn.query_drop(format!("USE `{}`", name.replace('`', "``"))).await {
                    *lock(&self.database) = None;
                    self.in_transaction.store(false, Ordering::SeqCst);
                    let back = lock(&self.opts).db_name().map(str::to_string).unwrap_or_default();
                    return Err(format!(
                        "The connection was lost and reopened, but the database {name} can't be opened again ({}). Nothing ran; the editor is back in {back}.",
                        error_message(&e, &self.endpoint).trim_end_matches('.')
                    ));
                }
            }
            if self.in_transaction.swap(false, Ordering::SeqCst) {
                return Err(ROLLED_BACK.into());
            }
        }
        let conn = held.as_mut().expect("connected above");
        let result = f(conn).await;
        // A connection that went down with the command keeps the mark, for the next command to
        // report the transaction it took along
        if result.is_err() && !alive(conn).await {
            *held = None;
        } else {
            self.in_transaction.store(in_transaction_status(conn), Ordering::SeqCst);
        }
        result
    }

    /// The mark a cancel leaves for the script that is running, to be checked between statements
    /// and after each one: `KILL QUERY` makes some statements end without an error (`SLEEP`).
    pub(crate) fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }

    /// The server's id of the connection (the latest one, after a reconnect).
    pub(crate) fn connection_id(&self) -> u32 {
        self.id.load(Ordering::SeqCst)
    }

    /// Whether a transaction is open, as last noted. Never waits.
    pub(crate) fn in_transaction(&self) -> bool {
        self.in_transaction.load(Ordering::SeqCst)
    }

    /// Notes the database the connection is in now (`DATABASE()`), so a connection opened
    /// again goes back to it. The session's own database needs no note.
    pub(crate) fn note_database(&self, current: Option<String>) {
        let default = lock(&self.opts).db_name().map(str::to_string);
        *lock(&self.database) = current.filter(|name| Some(name) != default.as_ref());
    }

    /// The options to reconnect with, once the session picked its database.
    pub(crate) fn set_opts(&self, opts: Opts) {
        *lock(&self.opts) = opts;
    }

    /// Lets go of the connection; it closes once no running task holds it. A command that
    /// already holds it doesn't start `f` after this (see `run`), and `connection_id()` read
    /// after this returns names the connection such a command may still be running on.
    pub(crate) fn close(&self) {
        let closed = lock(&self.slot).take();
        drop(closed);
    }

    #[cfg(test)]
    pub(crate) fn set_in_transaction_for_test(&self, open: bool) {
        self.in_transaction.store(open, Ordering::SeqCst);
    }

    /// Makes the next `run` stop once it holds the connection, before checking it: it
    /// signals `reached` and goes on when `proceed` is signalled.
    #[cfg(test)]
    pub(crate) fn pause_next_run_for_test(&self, reached: Arc<tokio::sync::Notify>, proceed: Arc<tokio::sync::Notify>) {
        *lock(&self.test_pause) = Some((reached, proceed));
    }

    #[cfg(test)]
    async fn pause_for_test(&self) {
        let pause = lock(&self.test_pause).take();
        if let Some((reached, proceed)) = pause {
            reached.notify_one();
            proceed.notified().await;
        }
    }
}

/// Whether the server says a transaction is open on `conn`, as of the last reply it read.
pub(crate) fn in_transaction_status(conn: &Conn) -> bool {
    conn.last_ok_packet().is_some_and(|ok| ok.status_flags().contains(StatusFlags::SERVER_STATUS_IN_TRANS))
}

/// Whether the server still answers on `conn`.
async fn alive(conn: &mut Conn) -> bool {
    matches!(tokio::time::timeout(PING_TIMEOUT, conn.ping()).await, Ok(Ok(())))
}

fn session_not_found() -> String {
    "Session not found".to_string()
}

/// A mutex a panicking task left poisoned still guards a usable value.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
