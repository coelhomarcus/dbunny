//! The PostgreSQL driver: a session's connections (main, query editor, transaction),
//! reconnection and the SSH tunnel they go through.

pub mod connect;
pub mod session;
pub mod ssh;

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

pub use session::{Endpoint, Link, Notices, PgState};

use super::{Capabilities, Catalog, ConnectionInfo, Data, Driver, Engine};

/// One open session on a PostgreSQL server.
pub struct PostgresDriver {
    /// The connections; None once the session was closed.
    state: Mutex<Option<PgState>>,
    /// Where the session connects, to connect again after the connection is lost.
    endpoint: Endpoint,
    /// Only one command reconnects at a time; the others wait and use its connection.
    reconnect_lock: Mutex<()>,
    info: ConnectionInfo,
    capabilities: Capabilities,
}

#[async_trait]
impl Driver for PostgresDriver {
    fn engine(&self) -> Engine {
        Engine::Postgres
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn info(&self) -> ConnectionInfo {
        self.info.clone()
    }

    /// Drops the session's connections and its tunnel, which closes them. A query already
    /// running on a connection a command still holds ends on its own (or fails); it isn't
    /// waited for. Anything the session is asked afterwards answers "Session not found".
    async fn close(&self) {
        let closed = self.state.lock().await.take();
        drop(closed);
    }

    fn catalog(&self) -> &dyn Catalog {
        unimplemented!("the commands still reach the catalog through the PostgreSQL driver")
    }

    fn data(&self) -> &dyn Data {
        unimplemented!("the commands still reach the data through the PostgreSQL driver")
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}
