//! A transaction opened from the query editor and kept open across runs.

use std::sync::Arc;

use async_trait::async_trait;
use tokio_postgres::error::SqlState;
use tokio_postgres::Client;

use super::connect::open_connection;
use super::data::db_message;
use super::{Link, Notices, PostgresDriver};
use crate::db::Transactions;

#[async_trait]
impl Transactions for PostgresDriver {
    /// Opens a transaction on a connection of its own: every editor run joins it until
    /// commit or rollback, while table views keep using the main connection.
    async fn begin(&self) -> Result<(), String> {
        if self.in_transaction().await? {
            return Err("A transaction is already open".into());
        }
        let link = self.link().await?;
        let notices = self.notices().await?;
        let client = start_transaction(&link, &notices).await?;

        let mut pg = self.lock().await;
        let session = pg.as_mut().ok_or("Session not found")?;
        if session.transaction.as_ref().is_some_and(|tx| !tx.is_closed()) {
            return Err("A transaction is already open".into());
        }
        session.transaction = Some(Arc::new(client));
        Ok(())
    }

    async fn commit(&self) -> Result<(), String> {
        let client = take_transaction(self).await?;
        finish_commit(&client).await
    }

    /// Rolls back. Closing the connection would roll back too, so a broken connection isn't an error.
    async fn rollback(&self) -> Result<(), String> {
        let client = take_transaction(self).await?;
        let _ = client.batch_execute("ROLLBACK").await;
        Ok(())
    }

    async fn is_open(&self) -> Result<bool, String> {
        self.in_transaction().await
    }
}

pub(super) async fn start_transaction(link: &Link, notices: &Notices) -> Result<Client, String> {
    let client = open_connection(link, notices)
        .await
        .map_err(|e| format!("Couldn't open the transaction connection: {}", e))?;
    client.batch_execute("BEGIN").await.map_err(|e| db_message(&e))?;
    Ok(client)
}

async fn take_transaction(driver: &PostgresDriver) -> Result<Arc<Client>, String> {
    let mut pg = driver.lock().await;
    let session = pg.as_mut().ok_or("Session not found")?;
    session.transaction.take().ok_or_else(|| "No transaction is open".to_string())
}

/// Commits, unless a statement failed: Postgres would silently roll back on COMMIT
/// then, so that case is reported as an error. The connection closes either way.
pub(crate) async fn finish_commit(client: &Client) -> Result<(), String> {
    if let Err(e) = client.simple_query("SELECT 1").await {
        if e.code() == Some(&SqlState::IN_FAILED_SQL_TRANSACTION) {
            let _ = client.batch_execute("ROLLBACK").await;
            return Err("A statement in the transaction failed, so it was rolled back instead of committed".into());
        }
        return Err(db_message(&e));
    }
    client
        .batch_execute("COMMIT")
        .await
        .map_err(|e| format!("Commit failed, nothing was saved: {}", db_message(&e)))
}

/// Tests that need a real server, see `console::server_tests`.
#[cfg(test)]
mod server_tests {
    use super::*;
    use crate::db::postgres::connect::connection_config;

    fn link() -> Link {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL")
            .expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        let url = url::Url::parse(&url).unwrap();
        let (config, tls) = connection_config(&url).unwrap();
        Link { config, tls }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn read_only_connections_reject_writes_on_every_connection() {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL").unwrap();
        let separator = if url.contains('?') { '&' } else { '?' };
        let url = url::Url::parse(&format!("{url}{separator}options=-c%20default_transaction_read_only%3Don")).unwrap();
        let (config, tls) = connection_config(&url).unwrap();
        let link = Link { config, tls };
        let notices: Notices = Default::default();

        let main = open_connection(&link, &notices).await.unwrap();
        let shown: String = main.query_one("SHOW transaction_read_only", &[]).await.unwrap().get(0);
        assert_eq!(shown, "on");
        let err = main.batch_execute("CREATE TEMP TABLE nope (v int)").await.unwrap_err();
        assert_eq!(err.code(), Some(&SqlState::READ_ONLY_SQL_TRANSACTION));

        // The editor's transaction connection is opened from the same link, so it's read-only too
        let tx = start_transaction(&link, &notices).await.unwrap();
        let err = tx.batch_execute("CREATE TEMP TABLE nope (v int)").await.unwrap_err();
        assert_eq!(err.code(), Some(&SqlState::READ_ONLY_SQL_TRANSACTION));
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn a_transaction_connection_is_invisible_to_others_until_commit() {
        let link = link();
        let notices: Notices = Default::default();
        let main = open_connection(&link, &notices).await.unwrap();
        main.batch_execute("DROP TABLE IF EXISTS dbunny_tx_test; CREATE TABLE dbunny_tx_test (v int)").await.unwrap();

        let tx = start_transaction(&link, &notices).await.unwrap();
        tx.batch_execute("INSERT INTO dbunny_tx_test VALUES (1)").await.unwrap();
        let outside: i64 = main.query_one("SELECT count(*) FROM dbunny_tx_test", &[]).await.unwrap().get(0);
        assert_eq!(outside, 0, "uncommitted rows must not be visible");
        finish_commit(&tx).await.unwrap();
        let after: i64 = main.query_one("SELECT count(*) FROM dbunny_tx_test", &[]).await.unwrap().get(0);
        assert_eq!(after, 1);

        // A failed statement turns COMMIT into an error, and nothing is saved
        let tx = start_transaction(&link, &notices).await.unwrap();
        tx.batch_execute("INSERT INTO dbunny_tx_test VALUES (2)").await.unwrap();
        assert!(tx.batch_execute("SELECT 1/0").await.is_err());
        let err = finish_commit(&tx).await.unwrap_err();
        assert!(err.contains("rolled back"), "{err}");
        let total: i64 = main.query_one("SELECT count(*) FROM dbunny_tx_test", &[]).await.unwrap().get(0);
        assert_eq!(total, 1);

        main.batch_execute("DROP TABLE dbunny_tx_test").await.unwrap();
    }
}
