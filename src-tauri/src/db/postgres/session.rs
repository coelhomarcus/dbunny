use std::sync::Arc;
use tokio::sync::MutexGuard;
use tokio_postgres::Client;

use super::PostgresDriver;
use crate::db::model::Notice;

/// The connections of an open session. None in the driver once it was closed.
pub struct PgState {
    // Shared so commands can use the client without holding the session's lock
    pub client: Arc<Client>,
    /// How to reach the server again: opening the transaction connection, cancelling queries.
    pub link: Link,
    /// Messages like `RAISE NOTICE`, from every connection of the session.
    pub notices: Notices,
    /// Separate connection for a transaction opened from the query editor, so table views
    /// and the explorer keep working outside it. None when no transaction is open.
    pub transaction: Option<Arc<Client>>,
    /// The query editor's own connection, opened on first use: what's typed there (BEGIN,
    /// SET, a long query) stays there instead of reaching table views and the explorer.
    pub editor: Option<Arc<Client>>,
    /// SSH tunnel the connections go through; held only to keep it open, and closed
    /// when the session is dropped.
    pub tunnel: Option<Arc<super::ssh::Tunnel>>,
}

/// The connection URL and SSH settings a session was opened with.
#[derive(Clone)]
pub struct Endpoint {
    pub url: url::Url,
    pub ssh: Option<super::ssh::SshConfig>,
    /// The app's SSH known hosts file, when connecting through SSH.
    pub known_hosts: Option<std::path::PathBuf>,
}

#[derive(Clone)]
pub struct Link {
    pub config: tokio_postgres::Config,
    pub tls: postgres_native_tls::MakeTlsConnector,
}

pub type Notices = Arc<std::sync::Mutex<Vec<Notice>>>;

impl PostgresDriver {
    /// The session's connections, None once it was closed. Held only briefly, never
    /// across a wait on the server.
    pub(crate) async fn lock(&self) -> MutexGuard<'_, Option<PgState>> {
        self.state.lock().await
    }

    /// Client of a session, reconnecting first if the connection was lost (server
    /// restart, network change, sleep). The session's lock is released before any wait,
    /// so a slow query or reconnect on one connection doesn't block the others.
    pub async fn client(&self) -> Result<Arc<Client>, String> {
        let client = {
            let state = self.state.lock().await;
            let session = state.as_ref().ok_or_else(|| "Session not found".to_string())?;
            Arc::clone(&session.client)
        };
        if !client.is_closed() {
            return Ok(client);
        }

        let _reconnecting = self.reconnect_lock.lock().await;
        // Another command may have reconnected while this one waited
        let (client, endpoint, link, notices, tunnel) = {
            let state = self.state.lock().await;
            let session = state.as_ref().ok_or_else(|| "Session not found".to_string())?;
            (
                Arc::clone(&session.client),
                self.endpoint.clone(),
                session.link.clone(),
                Arc::clone(&session.notices),
                session.tunnel.clone(),
            )
        };
        if !client.is_closed() {
            return Ok(client);
        }

        // A tunnel that is still up is reused; otherwise everything is opened again
        let (client, link, tunnel) = match tunnel {
            Some(tunnel) if !tunnel.is_alive() => super::connect::establish(&endpoint, &notices, None).await?,
            tunnel => {
                let client = super::connect::open_connection(&link, &notices)
                    .await
                    .map_err(|e| format!("The connection was lost and reconnecting failed: {e}"))?;
                (client, link, tunnel)
            }
        };
        let client = Arc::new(client);
        let mut state = self.state.lock().await;
        let session = state.as_mut().ok_or_else(|| "Session not found".to_string())?;
        session.client = Arc::clone(&client);
        session.link = link;
        session.tunnel = tunnel;
        Ok(client)
    }

    /// Client for the query editor: the transaction connection while one is open, otherwise
    /// the editor's own connection (opened, or opened again, here). A lost transaction
    /// connection can't be resumed: its transaction is gone, so it's dropped and reported,
    /// and the next run goes on without it.
    pub async fn editor_client(&self) -> Result<(Arc<Client>, bool), String> {
        if let Some(client) = self.open_editor().await? {
            return Ok(client);
        }
        // Reconnecting the main connection first also refreshes the link (a new SSH tunnel port)
        self.client().await?;
        let _opening = self.reconnect_lock.lock().await;
        // Another command may have opened it while this one waited
        if let Some(client) = self.open_editor().await? {
            return Ok(client);
        }
        let (link, notices) = (self.link().await?, self.notices().await?);
        let client = Arc::new(
            super::connect::open_connection(&link, &notices)
                .await
                .map_err(|e| format!("Couldn't open the editor's connection: {e}"))?,
        );
        let mut state = self.state.lock().await;
        let session = state.as_mut().ok_or_else(|| "Session not found".to_string())?;
        session.editor = Some(Arc::clone(&client));
        Ok((client, false))
    }

    /// The editor's connection that is up: the transaction's, else its own; None when it
    /// has none yet (or lost it).
    async fn open_editor(&self) -> Result<Option<(Arc<Client>, bool)>, String> {
        let mut state = self.state.lock().await;
        let session = state.as_mut().ok_or_else(|| "Session not found".to_string())?;
        if let Some(tx) = &session.transaction {
            if !tx.is_closed() {
                return Ok(Some((Arc::clone(tx), true)));
            }
            session.transaction = None;
            return Err("The connection running the transaction was lost, so the transaction was rolled back".into());
        }
        Ok(session.editor.as_ref().filter(|c| !c.is_closed()).map(|c| (Arc::clone(c), false)))
    }

    /// What the editor may be running right now (to cancel it), without opening anything.
    pub async fn running_editor(&self) -> Result<Option<Arc<Client>>, String> {
        let state = self.state.lock().await;
        let session = state.as_ref().ok_or_else(|| "Session not found".to_string())?;
        Ok(session.transaction.as_ref().or(session.editor.as_ref()).filter(|c| !c.is_closed()).cloned())
    }

    /// Whether a transaction opened from the editor is still going.
    pub async fn in_transaction(&self) -> Result<bool, String> {
        let state = self.state.lock().await;
        let session = state.as_ref().ok_or_else(|| "Session not found".to_string())?;
        Ok(session.transaction.as_ref().is_some_and(|tx| !tx.is_closed()))
    }

    pub async fn notices(&self) -> Result<Notices, String> {
        self.state
            .lock()
            .await
            .as_ref()
            .map(|session| Arc::clone(&session.notices))
            .ok_or_else(|| "Session not found".to_string())
    }

    pub async fn link(&self) -> Result<Link, String> {
        self.state
            .lock()
            .await
            .as_ref()
            .map(|session| session.link.clone())
            .ok_or_else(|| "Session not found".to_string())
    }

    /// Where the session connects; the same for its whole life.
    pub fn endpoint(&self) -> Endpoint {
        self.endpoint.clone()
    }

    #[cfg(test)]
    pub(crate) async fn set_transaction_for_test(&self, transaction: Option<Arc<Client>>) {
        self.state.lock().await.as_mut().unwrap().transaction = transaction;
    }
}

/// Tests that need a real server, see `query::server_tests`.
#[cfg(test)]
mod server_tests {
    use super::*;
    use crate::commands::state::AppState;

    fn test_endpoint() -> Endpoint {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL")
            .expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        Endpoint { url: url::Url::parse(&url).unwrap(), ssh: None, known_hosts: None }
    }

    /// Ends a connection from the server side, like a restart or a network drop would.
    async fn kill(client: &Client) {
        let pid: i32 = client.query_one("SELECT pg_backend_pid()", &[]).await.unwrap().get(0);
        // A separate connection does the terminating
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL").unwrap();
        let (admin, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(connection);
        admin.execute("SELECT pg_terminate_backend($1)", &[&pid]).await.unwrap();
        for _ in 0..50 {
            if client.is_closed() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("the connection didn't close");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn disconnecting_during_a_running_query_does_not_hang() {
        let state = AppState::new([0; 32]);
        let driver = PostgresDriver::open(test_endpoint(), None).await.unwrap();
        let id = state.register(driver.clone()).await.unwrap();
        let (editor, _) = driver.editor_client().await.unwrap();
        let running = tokio::spawn(async move { editor.simple_query("SELECT pg_sleep(1)").await.map(|_| ()) });
        state.remove(&id).await.unwrap().close().await;
        let finished = tokio::time::timeout(std::time::Duration::from_secs(5), running).await;
        assert!(finished.is_ok(), "the running query must end within 5s");
        assert_eq!(state.driver(&id).await.err().unwrap(), "Session not found");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn a_lost_connection_is_replaced_on_the_next_command() {
        let driver = PostgresDriver::open(test_endpoint(), None).await.unwrap();
        let before = driver.client().await.unwrap();
        kill(&before).await;

        let after = driver.client().await.unwrap();
        assert!(!Arc::ptr_eq(&before, &after), "a new connection must replace the dead one");
        let value: i32 = after.query_one("SELECT 1", &[]).await.unwrap().get(0);
        assert_eq!(value, 1);
        // Later commands keep the new connection
        assert!(Arc::ptr_eq(&after, &driver.client().await.unwrap()));
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn several_commands_reconnect_only_once() {
        let driver = PostgresDriver::open(test_endpoint(), None).await.unwrap();
        let before = driver.client().await.unwrap();
        kill(&before).await;

        let tasks: Vec<_> = (0..5)
            .map(|_| {
                let driver = Arc::clone(&driver);
                tokio::spawn(async move { driver.client().await.unwrap() })
            })
            .collect();
        let mut clients = Vec::new();
        for task in tasks {
            clients.push(task.await.unwrap());
        }
        assert!(clients.windows(2).all(|pair| Arc::ptr_eq(&pair[0], &pair[1])), "all commands share one new connection");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn a_lost_transaction_is_reported_once_then_the_editor_goes_on() {
        let driver = PostgresDriver::open(test_endpoint(), None).await.unwrap();
        let link = driver.link().await.unwrap();
        let notices = driver.notices().await.unwrap();
        let tx = Arc::new(super::super::connect::open_connection(&link, &notices).await.unwrap());
        tx.batch_execute("BEGIN").await.unwrap();
        driver.set_transaction_for_test(Some(Arc::clone(&tx))).await;
        assert!(driver.editor_client().await.unwrap().1);

        kill(&tx).await;
        let err = driver.editor_client().await.err().unwrap();
        assert!(err.contains("rolled back"), "{err}");
        let (_, in_transaction) = driver.editor_client().await.unwrap();
        assert!(!in_transaction);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn the_editor_keeps_its_session_state_to_itself() {
        let driver = PostgresDriver::open(test_endpoint(), None).await.unwrap();
        let (editor, in_transaction) = driver.editor_client().await.unwrap();
        assert!(!in_transaction);
        let main = driver.client().await.unwrap();
        let backend = |c: Arc<Client>| async move { c.query_one("SELECT pg_backend_pid()", &[]).await.unwrap().get::<_, i32>(0) };
        assert_ne!(backend(Arc::clone(&editor)).await, backend(Arc::clone(&main)).await, "two connections");

        // A BEGIN and a SET typed in the editor don't reach the connection table views use
        editor.batch_execute("BEGIN; SET LOCAL application_name = 'typed'; SET search_path = pg_catalog").await.unwrap();
        let path: String = main.query_one("SHOW search_path", &[]).await.unwrap().get(0);
        assert_ne!(path, "pg_catalog");
        let main_pid = backend(Arc::clone(&main)).await;
        let main_state: String = editor
            .query_one("SELECT state FROM pg_stat_activity WHERE pid = $1", &[&main_pid])
            .await
            .unwrap()
            .get(0);
        assert_eq!(main_state, "idle", "the main connection isn't inside the editor's transaction");
        editor.batch_execute("ROLLBACK").await.unwrap();

        // The same connection comes back each time, and a new one after it was lost
        assert!(Arc::ptr_eq(&driver.editor_client().await.unwrap().0, &editor));
        kill(&editor).await;
        let (again, _) = driver.editor_client().await.unwrap();
        assert!(!Arc::ptr_eq(&again, &editor));
        let one: i32 = again.query_one("SELECT 1", &[]).await.unwrap().get(0);
        assert_eq!(one, 1);
        assert!(driver.running_editor().await.unwrap().is_some());
    }
}
