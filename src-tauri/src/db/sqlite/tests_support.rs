//! Throwaway SQLite files for the tests, deleted when the test ends.

use std::path::PathBuf;

use uuid::Uuid;

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
