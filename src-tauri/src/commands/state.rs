use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::Client;

use super::storage::SavedConnection;

pub struct Session {
    // Shared so commands can use the client without holding the sessions lock
    pub client: Arc<Client>,
    /// How to reach the server again: opening the transaction connection, cancelling queries.
    pub link: Link,
    /// Messages like `RAISE NOTICE`, from every connection of the session.
    pub notices: Notices,
    /// Separate connection for a transaction opened from the query editor, so table views
    /// and the explorer keep working outside it. None when no transaction is open.
    pub transaction: Option<Arc<Client>>,
    /// SSH tunnel the connections go through; held only to keep it open, and closed
    /// when the session is dropped.
    pub tunnel: Option<Arc<super::ssh::Tunnel>>,
    /// Where the session connects, to connect again after the connection is lost.
    pub endpoint: Endpoint,
    /// Only one command reconnects at a time; the others wait and use its connection.
    pub reconnect_lock: Arc<Mutex<()>>,
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

/// A notice, warning or other non-error message sent by the server.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub severity: String,
    pub message: String,
    pub detail: Option<String>,
    pub hint: Option<String>,
}

pub type Notices = Arc<std::sync::Mutex<Vec<Notice>>>;

pub struct AppState {
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
    pub saved_connections: Arc<Mutex<Vec<SavedConnection>>>,
    pub encryption_key: [u8; 32],
}

impl AppState {
    pub fn new(encryption_key: [u8; 32]) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            saved_connections: Arc::new(Mutex::new(Vec::new())),
            encryption_key,
        }
    }

    /// Client of a session, reconnecting first if the connection was lost (server
    /// restart, network change, sleep). The sessions lock is released before any wait,
    /// so a slow query or reconnect on one connection doesn't block the others.
    pub async fn client(&self, session_id: &str) -> Result<Arc<Client>, String> {
        let (client, lock) = {
            let sessions = self.sessions.lock().await;
            let session = sessions.get(session_id).ok_or_else(|| "Session not found".to_string())?;
            (Arc::clone(&session.client), Arc::clone(&session.reconnect_lock))
        };
        if !client.is_closed() {
            return Ok(client);
        }

        let _reconnecting = lock.lock().await;
        // Another command may have reconnected while this one waited
        let (client, endpoint, link, notices, tunnel) = {
            let sessions = self.sessions.lock().await;
            let session = sessions.get(session_id).ok_or_else(|| "Session not found".to_string())?;
            (
                Arc::clone(&session.client),
                session.endpoint.clone(),
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
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(session_id).ok_or_else(|| "Session not found".to_string())?;
        session.client = Arc::clone(&client);
        session.link = link;
        session.tunnel = tunnel;
        Ok(client)
    }

    /// Client for the query editor: the transaction connection while one is open.
    /// A lost transaction connection can't be resumed: its transaction is gone, so it's
    /// dropped and reported, and the next run uses the main connection again.
    pub async fn editor_client(&self, session_id: &str) -> Result<(Arc<Client>, bool), String> {
        {
            let mut sessions = self.sessions.lock().await;
            let session = sessions.get_mut(session_id).ok_or_else(|| "Session not found".to_string())?;
            if let Some(tx) = &session.transaction {
                if !tx.is_closed() {
                    return Ok((Arc::clone(tx), true));
                }
                session.transaction = None;
                return Err("The connection running the transaction was lost, so the transaction was rolled back".into());
            }
        }
        Ok((self.client(session_id).await?, false))
    }

    pub async fn notices(&self, session_id: &str) -> Result<Notices, String> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .map(|session| Arc::clone(&session.notices))
            .ok_or_else(|| "Session not found".to_string())
    }

    pub async fn link(&self, session_id: &str) -> Result<Link, String> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .map(|session| session.link.clone())
            .ok_or_else(|| "Session not found".to_string())
    }
}

/// Tests that need a real server, see `query::server_tests`.
#[cfg(test)]
mod server_tests {
    use super::*;
    use crate::commands::connect::establish;

    async fn session_state() -> (AppState, String) {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL")
            .expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        let endpoint = Endpoint { url: url::Url::parse(&url).unwrap(), ssh: None, known_hosts: None };
        let notices: Notices = Default::default();
        let (client, link, tunnel) = establish(&endpoint, &notices, None).await.unwrap();
        let state = AppState {
            sessions: Default::default(),
            saved_connections: Default::default(),
            encryption_key: [0; 32],
        };
        let session = Session {
            client: Arc::new(client),
            link,
            notices,
            transaction: None,
            tunnel,
            endpoint,
            reconnect_lock: Default::default(),
        };
        state.sessions.lock().await.insert("s".into(), session);
        (state, "s".into())
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
    async fn a_lost_connection_is_replaced_on_the_next_command() {
        let (state, id) = session_state().await;
        let before = state.client(&id).await.unwrap();
        kill(&before).await;

        let after = state.client(&id).await.unwrap();
        assert!(!Arc::ptr_eq(&before, &after), "a new connection must replace the dead one");
        let value: i32 = after.query_one("SELECT 1", &[]).await.unwrap().get(0);
        assert_eq!(value, 1);
        // Later commands keep the new connection
        assert!(Arc::ptr_eq(&after, &state.client(&id).await.unwrap()));
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn several_commands_reconnect_only_once() {
        let (state, id) = session_state().await;
        let before = state.client(&id).await.unwrap();
        kill(&before).await;

        let state = Arc::new(state);
        let tasks: Vec<_> = (0..5)
            .map(|_| {
                let state = Arc::clone(&state);
                let id = id.clone();
                tokio::spawn(async move { state.client(&id).await.unwrap() })
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
        let (state, id) = session_state().await;
        let link = state.link(&id).await.unwrap();
        let notices = state.notices(&id).await.unwrap();
        let tx = Arc::new(crate::commands::connect::open_connection(&link, &notices).await.unwrap());
        tx.batch_execute("BEGIN").await.unwrap();
        state.sessions.lock().await.get_mut(&id).unwrap().transaction = Some(Arc::clone(&tx));
        assert!(state.editor_client(&id).await.unwrap().1);

        kill(&tx).await;
        let err = state.editor_client(&id).await.err().unwrap();
        assert!(err.contains("rolled back"), "{err}");
        let (_, in_transaction) = state.editor_client(&id).await.unwrap();
        assert!(!in_transaction);
    }
}
