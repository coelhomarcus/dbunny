//! The server around the database: its other databases, schemas, roles and what it's
//! doing right now.

use std::sync::Arc;

use async_trait::async_trait;
use tokio_postgres::Client;

use super::data::db_message;
use super::objects::{
    create_database_sql, create_schema_sql, drop_database_sql, drop_schema_sql, rename_schema_sql, run,
};
use super::session::Endpoint;
use super::PostgresDriver;
use crate::db::model::{DatabaseInfo, RoleInfo, ServerActivity, ServerSession};
use crate::db::{Driver, ServerAdmin};

#[async_trait]
impl ServerAdmin for PostgresDriver {
    async fn databases(&self) -> Result<Vec<DatabaseInfo>, String> {
        let client = self.client().await?;
        list_databases(&client).await
    }

    /// Another database of the same server, with the same login, SSL, read-only and SSH
    /// settings. The SSH host key was trusted when this session opened, so it isn't asked
    /// for again.
    async fn open_database(&self, name: &str) -> Result<Arc<dyn Driver>, String> {
        let endpoint = endpoint_for_database(&self.endpoint(), name)?;
        let driver: Arc<dyn Driver> = PostgresDriver::open(endpoint, None).await?;
        Ok(driver)
    }

    /// CREATE and DROP DATABASE refuse to run in a transaction, so they go alone in one message.
    async fn create_database(&self, name: &str) -> Result<(), String> {
        run(self, &create_database_sql(name)?).await
    }

    async fn drop_database(&self, name: &str, force: bool) -> Result<(), String> {
        run(self, &drop_database_sql(name, force)).await
    }

    async fn create_schema(&self, name: &str) -> Result<(), String> {
        run(self, &create_schema_sql(name)?).await
    }

    async fn rename_schema(&self, name: &str, new_name: &str) -> Result<(), String> {
        run(self, &rename_schema_sql(name, new_name)?).await
    }

    async fn drop_schema(&self, name: &str, cascade: bool) -> Result<(), String> {
        run(self, &drop_schema_sql(name, cascade)).await
    }

    async fn roles(&self) -> Result<Vec<RoleInfo>, String> {
        let client = self.client().await?;
        roles(&client).await.map_err(|e| db_message(&e))
    }

    async fn activity(&self, all_databases: bool) -> Result<ServerActivity, String> {
        let client = self.client().await?;
        server_activity(&client, all_databases).await.map_err(|e| db_message(&e))
    }

    async fn signal_backend(&self, pid: i32, terminate: bool) -> Result<bool, String> {
        let client = self.client().await?;
        signal(&client, pid, terminate).await
    }
}

/// The endpoint of an open session, pointed at another database of the same server.
pub(super) fn endpoint_for_database(endpoint: &Endpoint, database: &str) -> Result<Endpoint, String> {
    if database.trim().is_empty() {
        return Err("Choose a database".into());
    }
    let mut url = endpoint.url.clone();
    url.path_segments_mut()
        .map_err(|_| "Can't change the database of this connection".to_string())?
        .clear()
        .push(database);
    Ok(Endpoint { url, ..endpoint.clone() })
}

async fn list_databases(client: &tokio_postgres::Client) -> Result<Vec<DatabaseInfo>, String> {
    let rows = client
        .query(
            "SELECT datname::text AS name, \
             CASE WHEN has_database_privilege(datname, 'CONNECT') THEN pg_database_size(datname) END AS size_bytes, \
             datname = current_database() AS is_current, has_database_privilege(datname, 'CONNECT') AS can_connect \
             FROM pg_database WHERE NOT datistemplate AND datallowconn ORDER BY datname",
            &[],
        )
        .await
        .map_err(|e| super::data::db_message(&e))?;
    Ok(rows
        .iter()
        .map(|r| DatabaseInfo {
            name: r.get("name"),
            size_bytes: r.get("size_bytes"),
            is_current: r.get("is_current"),
            can_connect: r.get("can_connect"),
        })
        .collect())
}

/// What the server is doing right now: its sessions (pg_stat_activity), which of them
/// block others.
async fn server_activity(client: &Client, all_databases: bool) -> Result<ServerActivity, tokio_postgres::Error> {
    // backend_type (PG 10+) is read through to_jsonb so older servers still answer
    let rows = client
        .query(
            "SELECT a.pid, a.usename::text AS user_name, a.datname::text AS database, \
             NULLIF(a.application_name, '') AS application, host(a.client_addr) AS client, a.state, \
             CASE WHEN a.wait_event IS NOT NULL THEN a.wait_event_type || ': ' || a.wait_event END AS waiting_for, \
             EXTRACT(EPOCH FROM now() - a.query_start)::float8 AS query_seconds, \
             EXTRACT(EPOCH FROM now() - a.xact_start)::float8 AS transaction_seconds, \
             a.query, pg_blocking_pids(a.pid) AS blocked_by, a.pid = pg_backend_pid() AS is_self \
             FROM pg_stat_activity a \
             WHERE COALESCE(to_jsonb(a) ->> 'backend_type', 'client backend') = 'client backend' \
               AND ($1 OR a.datname = current_database()) \
             ORDER BY a.state = 'active' DESC, a.query_start DESC NULLS LAST",
            &[&all_databases],
        )
        .await?;
    let limited_view: bool = client
        .query_one(
            "SELECT NOT (pg_has_role(current_user, 'pg_read_all_stats', 'MEMBER') \
             OR (SELECT rolsuper FROM pg_roles WHERE rolname = current_user))",
            &[],
        )
        .await
        .map(|r| r.get(0))
        .unwrap_or(false);
    Ok(ServerActivity {
        sessions: rows
            .iter()
            .map(|r| ServerSession {
                pid: r.get("pid"),
                user: r.get("user_name"),
                database: r.get("database"),
                application: r.get("application"),
                client: r.get("client"),
                state: r.get("state"),
                waiting_for: r.get("waiting_for"),
                query_seconds: r.get("query_seconds"),
                transaction_seconds: r.get("transaction_seconds"),
                query: r.get("query"),
                blocked_by: r.get("blocked_by"),
                is_self: r.get("is_self"),
            })
            .collect(),
        limited_view,
    })
}

async fn signal(client: &Client, pid: i32, terminate: bool) -> Result<bool, String> {
    let own: i32 = client.query_one("SELECT pg_backend_pid()", &[]).await.map_err(|e| db_message(&e))?.get(0);
    if pid == own {
        return Err("That's DBunny's own connection".into());
    }
    let sql = if terminate { "SELECT pg_terminate_backend($1)" } else { "SELECT pg_cancel_backend($1)" };
    client.query_one(sql, &[&pid]).await.map(|r| r.get(0)).map_err(|e| db_message(&e))
}

/// The server's roles, for the Roles page.
async fn roles(client: &Client) -> Result<Vec<RoleInfo>, tokio_postgres::Error> {
    // rolbypassrls exists from 9.5; read through to_jsonb like other newer columns
    let rows = client
        .query(
            "SELECT r.rolname::text AS name, r.rolcanlogin AS can_login, r.rolsuper AS superuser, \
             r.rolcreatedb AS create_db, r.rolcreaterole AS create_role, r.rolreplication AS replication, \
             COALESCE((to_jsonb(r) ->> 'rolbypassrls')::boolean, false) AS bypass_rls, \
             r.rolconnlimit AS connection_limit, to_char(r.rolvaliduntil, 'YYYY-MM-DD HH24:MI TZ') AS valid_until, \
             ARRAY(SELECT g.rolname::text FROM pg_auth_members m JOIN pg_roles g ON g.oid = m.roleid \
                   WHERE m.member = r.oid ORDER BY g.rolname) AS member_of, \
             r.rolname = current_user AS is_current, r.rolname LIKE 'pg\\_%' AS is_builtin \
             FROM pg_roles r ORDER BY r.rolname LIKE 'pg\\_%', NOT r.rolcanlogin, r.rolname",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| RoleInfo {
            name: r.get("name"),
            can_login: r.get("can_login"),
            superuser: r.get("superuser"),
            create_db: r.get("create_db"),
            create_role: r.get("create_role"),
            replication: r.get("replication"),
            bypass_rls: r.get("bypass_rls"),
            connection_limit: r.get("connection_limit"),
            valid_until: r.get("valid_until"),
            member_of: r.get("member_of"),
            is_current: r.get("is_current"),
            is_builtin: r.get("is_builtin"),
        })
        .collect())
}

/// Tests that need a real server, see `db::postgres::console::server_tests`.
#[cfg(test)]
mod activity_server_tests {
    use super::*;

    async fn connect() -> Client {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL")
            .expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(connection);
        client
    }

    async fn pid(client: &Client) -> i32 {
        client.query_one("SELECT pg_backend_pid()", &[]).await.unwrap().get(0)
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn shows_running_and_blocked_sessions_and_can_stop_them() {
        let watcher = connect().await;
        let holder = connect().await;
        let waiter = std::sync::Arc::new(connect().await);
        let (holder_pid, waiter_pid) = (pid(&holder).await, pid(&waiter).await);

        watcher.batch_execute("DROP TABLE IF EXISTS dbunny_activity_test; CREATE TABLE dbunny_activity_test (v int); INSERT INTO dbunny_activity_test VALUES (1)").await.unwrap();
        holder.batch_execute("BEGIN; UPDATE dbunny_activity_test SET v = 2").await.unwrap();
        let blocked = {
            let waiter = std::sync::Arc::clone(&waiter);
            tokio::spawn(async move { waiter.batch_execute("UPDATE dbunny_activity_test SET v = 3").await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        let activity = server_activity(&watcher, false).await.unwrap();
        let find = |p: i32| activity.sessions.iter().find(|s| s.pid == p).unwrap_or_else(|| panic!("no session {p}"));
        assert!(find(pid(&watcher).await).is_self);
        assert_eq!(find(holder_pid).state.as_deref(), Some("idle in transaction"));
        let waiting = find(waiter_pid);
        assert_eq!(waiting.blocked_by, vec![holder_pid]);
        assert!(waiting.waiting_for.as_deref().unwrap_or("").starts_with("Lock"), "{:?}", waiting.waiting_for);
        assert!(waiting.query.as_deref().unwrap_or("").contains("SET v = 3"));

        // Cancelling the waiting query frees it with an error; ending the holder releases its lock
        assert!(signal(&watcher, waiter_pid, false).await.unwrap());
        assert!(blocked.await.unwrap().is_err());
        assert!(signal(&watcher, holder_pid, true).await.unwrap());
        for _ in 0..40 {
            if holder.is_closed() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(holder.is_closed());

        let err = signal(&watcher, pid(&watcher).await, true).await.unwrap_err();
        assert!(err.contains("own connection"));
        watcher.batch_execute("DROP TABLE dbunny_activity_test").await.unwrap();
    }
}

/// Tests that need a real server, see `db::postgres::console::server_tests`.
#[cfg(test)]
mod roles_server_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn lists_roles_with_memberships() {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL").expect("set DBUNNY_TEST_DATABASE_URL");
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(connection);
        client
            .batch_execute(
                "DROP ROLE IF EXISTS dbunny_test_member; DROP ROLE IF EXISTS dbunny_test_group; \
                 CREATE ROLE dbunny_test_group NOLOGIN; \
                 CREATE ROLE dbunny_test_member LOGIN CONNECTION LIMIT 3 IN ROLE dbunny_test_group;",
            )
            .await
            .unwrap();
        let list = roles(&client).await.unwrap();
        let member = list.iter().find(|r| r.name == "dbunny_test_member").unwrap();
        assert!(member.can_login && !member.superuser);
        assert_eq!(member.connection_limit, 3);
        assert_eq!(member.member_of, vec!["dbunny_test_group"]);
        assert!(!list.iter().find(|r| r.name == "dbunny_test_group").unwrap().can_login);
        assert!(list.iter().any(|r| r.is_current));
        assert!(list.iter().filter(|r| r.is_builtin).all(|r| r.name.starts_with("pg_")));
        // Built-in roles come last
        let first_builtin = list.iter().position(|r| r.is_builtin).unwrap_or(list.len());
        assert!(list[first_builtin..].iter().all(|r| r.is_builtin));
        client.batch_execute("DROP ROLE dbunny_test_member; DROP ROLE dbunny_test_group").await.unwrap();
    }
}

/// Switching databases against a real server, see `console::server_tests`.
#[cfg(test)]
mod database_server_tests {
    use super::super::ssh::SshConfig;
    use super::*;
    use crate::commands::admin::open_database_session;
    use crate::commands::connect::{open_session, ConnectResponse};
    use crate::commands::state::AppState;
    use uuid::Uuid;

    /// What `open_session` does, keeping the driver to reach its connections.
    async fn open_kept(state: &AppState, endpoint: Endpoint, trust_host_key: Option<String>) -> (ConnectResponse, Arc<PostgresDriver>) {
        let driver = PostgresDriver::open(endpoint, trust_host_key).await.unwrap();
        let session = ConnectResponse::new(state.register(driver.clone()).await.unwrap(), driver.as_ref());
        (session, driver)
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL and a second database named dbunny_second"]
    async fn another_database_opens_as_its_own_session() {
        let url = url::Url::parse(&std::env::var("DBUNNY_TEST_DATABASE_URL").unwrap()).unwrap();
        let state = AppState {
            sessions: Default::default(),
            saved_connections: Default::default(),
            encryption_key: [0; 32],
        };
        let (first, first_driver) = open_kept(&state, Endpoint { url, ssh: None, known_hosts: None }, None).await;

        let client = first_driver.client().await.unwrap();
        let databases = list_databases(&client).await.unwrap();
        let names: Vec<_> = databases.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"dbunny_second"), "{names:?}");
        assert!(!names.contains(&"template1"));
        assert!(databases.iter().find(|d| d.is_current).unwrap().size_bytes.unwrap() > 0);

        let second = open_database_session(&state, &first.session_id, "dbunny_second").await.unwrap();
        assert_eq!(second.database, "dbunny_second");
        assert_ne!(second.session_id, first.session_id);
        // Asked on the second session's connection: the database it's on
        let current: String = state.driver(&second.session_id).await.unwrap().admin().unwrap().databases().await.unwrap()
            .into_iter().find(|d| d.is_current).unwrap().name;
        assert_eq!(current, "dbunny_second");
        // The first session is untouched
        let current: String = client.query_one("SELECT current_database()::text", &[]).await.unwrap().get(0);
        assert_eq!(current, first.database);
    }

    /// The database switcher through an SSH tunnel: the second database opens through a
    /// tunnel of its own, and the host key trusted for the first needs no new confirmation.
    ///   DBUNNY_TEST_SSH=user:password@host:port
    ///   DBUNNY_TEST_SSH_DATABASE_URL=<database URL as seen from the SSH server>
    /// and a second database named dbunny_second.
    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_SSH and DBUNNY_TEST_SSH_DATABASE_URL"]
    async fn another_database_opens_through_the_same_tunnel_settings() {
        let ssh = std::env::var("DBUNNY_TEST_SSH").unwrap();
        let (auth, addr) = ssh.split_once('@').unwrap();
        let (user, password) = auth.split_once(':').unwrap();
        let (host, port) = addr.rsplit_once(':').unwrap();
        let ssh = SshConfig {
            host: host.into(),
            port: port.parse().unwrap(),
            user: user.into(),
            auth_method: "password".into(),
            password: password.into(),
            key_path: String::new(),
            passphrase: String::new(),
        };
        let known_hosts = std::env::temp_dir().join(format!("dbunny-switch-test-{}.json", Uuid::new_v4()));
        let endpoint = Endpoint {
            url: url::Url::parse(&std::env::var("DBUNNY_TEST_SSH_DATABASE_URL").unwrap()).unwrap(),
            ssh: Some(ssh),
            known_hosts: Some(known_hosts.clone()),
        };
        let state = AppState { sessions: Default::default(), saved_connections: Default::default(), encryption_key: [0; 32] };

        // First time: the host key is unknown, so the UI asks and retries with its fingerprint
        let err = open_session(&state, endpoint.clone(), None).await.err().expect("an unknown host key must be confirmed");
        let problem: serde_json::Value = serde_json::from_str(&err).unwrap_or_else(|_| panic!("not a host key error: {err}"));
        let fingerprint = problem["sshHostKey"]["fingerprint"].as_str().unwrap().to_string();
        let (first, first_driver) = open_kept(&state, endpoint, Some(fingerprint)).await;

        // The switcher: list the server's databases through the tunnel, open another one
        let client = first_driver.client().await.unwrap();
        let names: Vec<String> = list_databases(&client).await.unwrap().into_iter().map(|d| d.name).collect();
        assert!(names.iter().any(|n| n == "dbunny_second"), "{names:?}");
        let other = endpoint_for_database(&first_driver.endpoint(), "dbunny_second").unwrap();
        assert!(other.ssh.is_some(), "the SSH settings come along");
        // What `open_database_session` does (no host key passed), keeping the driver
        let (_, second_driver) = open_kept(&state, other, None).await;
        let current: String = second_driver.client().await.unwrap().query_one("SELECT current_database()::text", &[]).await.unwrap().get(0);
        assert_eq!(current, "dbunny_second");

        // Each session has its own tunnel; closing the first leaves the second working
        let local_port = |driver: Arc<PostgresDriver>| async move {
            driver.lock().await.as_ref().unwrap().tunnel.as_ref().unwrap().local_port
        };
        let (one, two) = (local_port(Arc::clone(&first_driver)).await, local_port(Arc::clone(&second_driver)).await);
        assert_ne!(one, two);
        state.sessions.lock().await.remove(&first.session_id);
        // The last hold on the first session: dropping it closes its tunnel
        drop(first_driver);
        let still: i32 = second_driver.client().await.unwrap().query_one("SELECT 1", &[]).await.unwrap().get(0);
        assert_eq!(still, 1);
        let _ = std::fs::remove_file(known_hosts);
    }
}
