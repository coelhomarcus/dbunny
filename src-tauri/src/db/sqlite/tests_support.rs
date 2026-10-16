//! Throwaway SQLite files for the tests, deleted when the test ends, and a way to test
//! statements that only stop when told to.

use std::future::Future;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;
use uuid::Uuid;

/// Runs the async test `body` like `#[tokio::test]` does, except that a failing test
/// doesn't wait for a statement still running on a blocking thread: the runtime is shut
/// down without waiting, so the test fails instead of hanging.
pub fn without_hanging(body: impl Future<Output = ()>) {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runtime.block_on(body)));
    runtime.shutdown_background();
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// Calls `stop` each `every` until `task` ends, and gives what it returned with how many
/// times `stop` was called: a stop that lands before the statement starts isn't the last
/// one. Fails the test once `limit` has passed.
pub async fn stop_until_done<T, F: Future<Output = ()>>(
    task: &mut JoinHandle<T>,
    every: Duration,
    limit: Duration,
    mut stop: impl FnMut() -> F,
) -> (T, usize) {
    let deadline = Instant::now() + limit;
    let mut stops = 0;
    loop {
        stop().await;
        stops += 1;
        match tokio::time::timeout(every, &mut *task).await {
            Ok(joined) => return (joined.expect("the task panicked"), stops),
            Err(_) => assert!(Instant::now() < deadline, "still running {limit:?} after being told to stop"),
        }
    }
}

/// A file named `dbunny-test-{uuid}.sqlite` in the temporary directory. Dropping it
/// deletes the file and the journal files SQLite may have left beside it.
pub struct TempDb {
    path: PathBuf,
}

impl TempDb {
    /// A database holding what `sql` creates.
    pub fn with(sql: &str) -> TempDb {
        let db = TempDb::missing();
        let conn = rusqlite::Connection::open(&db.path).unwrap();
        conn.execute_batch(sql).unwrap();
        db
    }

    /// A file holding exactly `bytes`.
    pub fn raw(bytes: &[u8]) -> TempDb {
        let db = TempDb::missing();
        std::fs::write(&db.path, bytes).unwrap();
        db
    }

    /// A path no file has yet, deleted at the end like the others if a test creates it.
    pub fn missing() -> TempDb {
        TempDb { path: TempDb::unused_path() }
    }

    /// A fresh path in the temporary directory, with no file.
    pub fn unused_path() -> PathBuf {
        std::env::temp_dir().join(format!("dbunny-test-{}.sqlite", Uuid::new_v4()))
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn file_name(&self) -> String {
        self.path.file_name().unwrap().to_string_lossy().into_owned()
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        for suffix in ["-journal", "-wal", "-shm"] {
            let mut side = self.path.clone().into_os_string();
            side.push(suffix);
            let _ = std::fs::remove_file(side);
        }
    }
}
