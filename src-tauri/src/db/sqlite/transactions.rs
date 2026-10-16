//! A transaction opened from the query editor, on the editor's connection, kept open across
//! runs. The tables' connection is separate, so it sees the changes only after the commit.

use async_trait::async_trait;

use super::errors::map_statement_error;
use super::SqliteDriver;
use crate::db::Transactions;

impl SqliteDriver {
    /// Runs `sql` on the editor's connection. SQLite's errors keep their text, such as
    /// "cannot start a transaction within a transaction"; a lock is another program's,
    /// since the editor's own transaction can't block it.
    async fn on_editor(&self, sql: &'static str) -> Result<(), String> {
        self.editor().run(move |c| c.execute_batch(sql).map_err(|e| map_statement_error(&e, false))).await
    }
}

#[async_trait]
impl Transactions for SqliteDriver {
    /// Every editor run joins the transaction until commit or rollback. Closing the
    /// session with it open discards it.
    async fn begin(&self) -> Result<(), String> {
        self.on_editor("BEGIN").await
    }

    async fn commit(&self) -> Result<(), String> {
        self.on_editor("COMMIT").await
    }

    async fn rollback(&self) -> Result<(), String> {
        self.on_editor("ROLLBACK").await
    }

    /// As of the editor's last run, without waiting for a statement it's running now.
    async fn is_open(&self) -> Result<bool, String> {
        Ok(self.editor().in_transaction())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use serde_json::json;

    use super::super::tests_support::TempDb;
    use super::super::OpenOptions;
    use super::*;
    use crate::db::model::RowChange;
    use crate::db::Driver;

    const HELD_BY_EDITOR: &str = "A transaction in the query editor is holding the database: commit or roll it back, then try again.";
    const HELD_BY_ANOTHER: &str = "The database is locked by another program. Try again when it's done.";

    async fn open(f: &TempDb) -> Arc<SqliteDriver> {
        let opts = OpenOptions { busy_timeout: Duration::from_millis(200), ..Default::default() };
        SqliteDriver::open(f.path(), opts).await.unwrap()
    }

    fn rename(id: i64, name: &str) -> Vec<RowChange> {
        let key = json!({ "id": id }).as_object().unwrap().clone();
        let updates = json!({ "name": name }).as_object().unwrap().clone();
        vec![RowChange { primary_key: key, updates }]
    }

    const PEOPLE: &str = "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT); INSERT INTO t VALUES (1, 'a');";

    #[tokio::test]
    async fn editor_transactions() {
        let f = TempDb::with(PEOPLE);
        let d = open(&f).await;
        let (tx, console) = (d.transactions().unwrap(), d.console().unwrap());
        let rows = || d.data().count_rows("main", "t");

        assert!(!tx.is_open().await.unwrap());
        tx.begin().await.unwrap();
        assert!(tx.is_open().await.unwrap());
        assert!(console.execute("INSERT INTO t VALUES (2, 'b')").await.unwrap().in_transaction);
        assert_eq!(rows().await.unwrap(), 1, "the tables don't see it before the commit");
        tx.commit().await.unwrap();
        assert!(!tx.is_open().await.unwrap());
        assert_eq!(rows().await.unwrap(), 2);

        tx.begin().await.unwrap();
        console.execute("INSERT INTO t VALUES (3, 'c')").await.unwrap();
        assert_eq!(console.execute("SELECT count(*) FROM t").await.unwrap().rows, vec![vec![json!(3)]], "the editor sees its own change");
        tx.rollback().await.unwrap();
        assert!(!console.execute("SELECT 1").await.unwrap().in_transaction);
        assert_eq!(rows().await.unwrap(), 2);

        // SQLite's own words for the wrong order
        tx.begin().await.unwrap();
        assert_eq!(tx.begin().await.err().unwrap(), "cannot start a transaction within a transaction");
        tx.rollback().await.unwrap();
        assert_eq!(tx.commit().await.err().unwrap(), "cannot commit - no transaction is active");
        assert_eq!(tx.rollback().await.err().unwrap(), "cannot rollback - no transaction is active");

        // A BEGIN typed in the editor is the same transaction
        assert!(console.execute("BEGIN").await.unwrap().in_transaction);
        assert!(tx.is_open().await.unwrap());
        assert!(!console.execute("INSERT INTO t VALUES (4, 'd'); COMMIT").await.unwrap().in_transaction);
        assert!(!tx.is_open().await.unwrap());
        assert_eq!(rows().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn closing_the_session_discards_its_transaction() {
        let f = TempDb::with(PEOPLE);
        let d = open(&f).await;
        d.transactions().unwrap().begin().await.unwrap();
        d.console().unwrap().execute("INSERT INTO t VALUES (2, 'b')").await.unwrap();
        d.close().await;
        let conn = rusqlite::Connection::open(f.path()).unwrap();
        let count: i64 = conn.query_row("SELECT count(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn a_save_waits_for_the_editors_transaction_then_explains() {
        let f = TempDb::with(PEOPLE);
        let d = open(&f).await;
        d.transactions().unwrap().begin().await.unwrap();
        d.console().unwrap().execute("INSERT INTO t VALUES (2, 'b')").await.unwrap();

        let started = Instant::now();
        let err = d.data().save_rows("main", "t", rename(1, "A")).await.err().unwrap();
        assert_eq!(err, HELD_BY_EDITOR);
        assert!(started.elapsed() >= Duration::from_millis(150), "it waited for the lock: {:?}", started.elapsed());

        d.transactions().unwrap().commit().await.unwrap();
        assert_eq!(d.data().save_rows("main", "t", rename(1, "A")).await.unwrap(), 1);
        let names = d.console().unwrap().execute("SELECT name FROM t ORDER BY id").await.unwrap().rows;
        assert_eq!(names, vec![vec![json!("A")], vec![json!("b")]]);
    }

    #[tokio::test]
    async fn another_program_holding_the_lock_gets_its_own_message() {
        let f = TempDb::with(PEOPLE);
        let d = open(&f).await;
        let other = rusqlite::Connection::open(f.path()).unwrap();
        other.execute_batch("BEGIN EXCLUSIVE").unwrap();

        assert_eq!(d.data().save_rows("main", "t", rename(1, "A")).await.err().unwrap(), HELD_BY_ANOTHER);
        let err = d.console().unwrap().execute("SELECT * FROM t").await.err().unwrap();
        let e: serde_json::Value = serde_json::from_str(&err).unwrap();
        assert_eq!((e["message"].as_str(), e["code"].as_str()), (Some(HELD_BY_ANOTHER), Some("SQLITE_BUSY")));
        // An editor transaction of its own doesn't change who holds the file
        d.transactions().unwrap().begin().await.unwrap();
        let err = d.console().unwrap().execute("SELECT * FROM t").await.err().unwrap();
        assert!(err.contains(HELD_BY_ANOTHER), "{err}");
        d.transactions().unwrap().rollback().await.unwrap();

        other.execute_batch("ROLLBACK").unwrap();
        assert_eq!(d.data().save_rows("main", "t", rename(1, "A")).await.unwrap(), 1);
    }
}
