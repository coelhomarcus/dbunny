//! The query editor's own connection. What's typed there (a transaction, SET, a long query)
//! stays on it instead of reaching the pool behind table views and the explorer.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use futures_util::future::BoxFuture;
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
    /// Whether a transaction is open, readable without waiting for the connection. The
    /// console sets it after each statement (Task 10); a connection lost while it is set
    /// reports the rolled back transaction once, on the next command.
    in_transaction: AtomicBool,
    /// Set by a cancel, for a script to stop before its next statement (Task 9).
    #[allow(dead_code)]
    cancelled: Arc<AtomicBool>,
    /// How to open the connection again.
    opts: Mutex<Opts>,
    /// For the messages of connection errors.
    endpoint: MysqlEndpoint,
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
            opts: Mutex::new(opts),
            endpoint,
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
        if let Some(conn) = held.as_mut() {
            if !alive(conn).await {
                *held = None;
            }
        }
        if held.is_none() {
            let opts = lock(&self.opts).clone();
            let conn = Conn::new(opts).await.map_err(|e| error_message(&e, &self.endpoint))?;
            self.id.store(conn.id(), Ordering::SeqCst);
            *held = Some(conn);
            if self.in_transaction.swap(false, Ordering::SeqCst) {
                return Err(ROLLED_BACK.into());
            }
        }
        let conn = held.as_mut().expect("connected above");
        let result = f(conn).await;
        if result.is_err() && !alive(conn).await {
            *held = None;
        }
        result
    }

    /// The server's id of the connection (the latest one, after a reconnect).
    pub(crate) fn connection_id(&self) -> u32 {
        self.id.load(Ordering::SeqCst)
    }

    /// Whether a transaction is open, as last noted. Never waits.
    #[allow(dead_code)] // read by the transactions (Task 10)
    pub(crate) fn in_transaction(&self) -> bool {
        self.in_transaction.load(Ordering::SeqCst)
    }

    /// The options to reconnect with, once the session picked its database.
    pub(super) fn set_opts(&self, opts: Opts) {
        *lock(&self.opts) = opts;
    }

    /// Lets go of the connection; it closes once no running task holds it.
    pub(crate) fn close(&self) {
        let closed = lock(&self.slot).take();
        drop(closed);
    }

    #[cfg(test)]
    pub(crate) fn set_in_transaction_for_test(&self, open: bool) {
        self.in_transaction.store(open, Ordering::SeqCst);
    }
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
