//! The transaction of the query editor, kept open on its connection across runs.

use async_trait::async_trait;
use mysql_async::prelude::Queryable;

use super::MysqlDriver;
use crate::db::mysql_common::errors::error_message;
use crate::db::Transactions;

impl MysqlDriver {
    /// Runs `sql` on the editor's connection, which keeps the transaction.
    async fn on_editor(&self, sql: &'static str) -> Result<(), String> {
        let endpoint = self.endpoint().clone();
        self.editor().run(move |conn| Box::pin(async move { conn.query_drop(sql).await.map_err(|e| error_message(&e, &endpoint)) })).await
    }
}

#[async_trait]
impl Transactions for MysqlDriver {
    async fn begin(&self) -> Result<(), String> {
        self.on_editor("START TRANSACTION").await
    }

    async fn commit(&self) -> Result<(), String> {
        self.on_editor("COMMIT").await
    }

    async fn rollback(&self) -> Result<(), String> {
        self.on_editor("ROLLBACK").await
    }

    /// As of the last command on the editor's connection; never waits for a running one.
    async fn is_open(&self) -> Result<bool, String> {
        Ok(self.editor().in_transaction())
    }
}

#[cfg(test)]
mod server_tests {
    use std::sync::Arc;

    use mysql_async::prelude::Queryable;

    use super::super::editor::ROLLED_BACK;
    use super::super::tests_support::{exec, query_one, raw_conn, TestDb};
    use super::super::MysqlDriver;
    use crate::db::Driver;

    const IMPLICIT_COMMIT: &str = "This statement committed the open transaction: MySQL commits before DDL.";

    async fn open() -> (TestDb, Arc<MysqlDriver>) {
        let db = TestDb::new().await;
        exec(&db.endpoint, "CREATE TABLE t (id INT PRIMARY KEY)").await;
        let driver = MysqlDriver::open(db.endpoint.clone(), None).await.unwrap();
        (db, driver)
    }

    /// Rows of `t` as another connection sees them.
    async fn rows_seen_elsewhere(db: &TestDb) -> i64 {
        query_one(&db.endpoint, "SELECT COUNT(*) FROM t").await.unwrap()
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn begin_commit_and_rollback_through_the_button() {
        let (db, d) = open().await;
        let (console, tx) = (d.console().unwrap(), d.transactions().expect("mysql has transactions"));
        assert!(!tx.is_open().await.unwrap());

        tx.begin().await.unwrap();
        assert!(tx.is_open().await.unwrap());
        let inserted = console.execute("INSERT INTO t VALUES (1)").await.unwrap();
        assert!(inserted.in_transaction, "a script run inside the transaction says so");
        // Reading, and failing, inside the transaction leave it open
        assert!(console.execute("SELECT * FROM t").await.unwrap().in_transaction);
        assert!(console.execute("SELECT * FROM nope").await.is_err());
        assert!(tx.is_open().await.unwrap());
        assert_eq!(rows_seen_elsewhere(&db).await, 0, "invisible to others until commit");

        tx.rollback().await.unwrap();
        assert!(!tx.is_open().await.unwrap());
        assert_eq!(rows_seen_elsewhere(&db).await, 0);
        assert!(!console.execute("SELECT * FROM t").await.unwrap().in_transaction);

        tx.begin().await.unwrap();
        console.execute("INSERT INTO t VALUES (2)").await.unwrap();
        tx.commit().await.unwrap();
        assert!(!tx.is_open().await.unwrap());
        assert_eq!(rows_seen_elsewhere(&db).await, 1);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn begin_typed_in_the_editor_counts_as_a_transaction() {
        let (_db, d) = open().await;
        let (console, tx) = (d.console().unwrap(), d.transactions().unwrap());
        let begun = console.execute("BEGIN").await.unwrap();
        assert!(begun.in_transaction && tx.is_open().await.unwrap());
        let ended = console.execute("COMMIT").await.unwrap();
        assert!(!ended.in_transaction && !tx.is_open().await.unwrap());
        assert!(ended.notices.is_empty(), "ending it yourself is no surprise");
        // ROLLBACK typed in the editor ends it too, also in the middle of a script
        let script = console.execute("START TRANSACTION; INSERT INTO t VALUES (9); ROLLBACK").await.unwrap();
        assert!(!script.in_transaction && script.notices.is_empty());
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn ddl_inside_a_transaction_reports_the_implicit_commit() {
        let (db, d) = open().await;
        let (console, tx) = (d.console().unwrap(), d.transactions().unwrap());
        tx.begin().await.unwrap();
        console.execute("INSERT INTO t VALUES (1)").await.unwrap();
        let result = console.execute("CREATE TABLE y (x INT)").await.unwrap();
        let notice = result.notices.first().expect("the commit is reported");
        assert_eq!((notice.severity.as_str(), notice.message.as_str()), ("Warning", IMPLICIT_COMMIT));
        assert!(!result.in_transaction && !tx.is_open().await.unwrap());
        assert_eq!(rows_seen_elsewhere(&db).await, 1, "the insert was committed");
        // Outside a transaction DDL says nothing
        assert!(console.execute("CREATE TABLE z (x INT)").await.unwrap().notices.is_empty());
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_dropped_editor_connection_reports_the_lost_transaction() {
        let (db, d) = open().await;
        let (console, tx) = (d.console().unwrap(), d.transactions().unwrap());
        tx.begin().await.unwrap();
        console.execute("INSERT INTO t VALUES (1)").await.unwrap();

        let mut other = raw_conn(&db.endpoint).await;
        other.query_drop(format!("KILL CONNECTION {}", d.editor().connection_id())).await.unwrap();
        assert!(tx.is_open().await.unwrap(), "nothing noticed the loss yet");

        assert_eq!(console.execute("SELECT 1").await.err().unwrap(), ROLLED_BACK);
        assert!(!tx.is_open().await.unwrap());
        let next = console.execute("SELECT 1").await.unwrap();
        assert_eq!(next.row_count, 1);
        assert_eq!(rows_seen_elsewhere(&db).await, 0, "the insert went with the transaction");
    }
}
