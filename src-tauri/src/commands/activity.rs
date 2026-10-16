//! What the server is doing right now: its sessions (pg_stat_activity), which of them
//! block others, and cancelling or ending one.

use tauri::State;
use tokio_postgres::Client;

use super::query::db_message;
use super::state::AppState;
use crate::db::model::{ServerActivity, ServerSession};

#[tauri::command]
pub async fn get_server_activity(
    state: State<'_, AppState>,
    session_id: String,
    all_databases: bool,
) -> Result<ServerActivity, String> {
    let client = state.client(&session_id).await?;
    server_activity(&client, all_databases).await.map_err(|e| db_message(&e))
}

pub(crate) async fn server_activity(client: &Client, all_databases: bool) -> Result<ServerActivity, tokio_postgres::Error> {
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

/// Cancels the session's running query (`terminate` false) or ends the session.
/// Refuses DBunny's own main connection, which would cut off the app.
#[tauri::command]
pub async fn signal_backend(
    state: State<'_, AppState>,
    session_id: String,
    pid: i32,
    terminate: bool,
) -> Result<bool, String> {
    let client = state.client(&session_id).await?;
    signal(&client, pid, terminate).await
}

pub(crate) async fn signal(client: &Client, pid: i32, terminate: bool) -> Result<bool, String> {
    let own: i32 = client.query_one("SELECT pg_backend_pid()", &[]).await.map_err(|e| db_message(&e))?.get(0);
    if pid == own {
        return Err("That's DBunny's own connection".into());
    }
    let sql = if terminate { "SELECT pg_terminate_backend($1)" } else { "SELECT pg_cancel_backend($1)" };
    client.query_one(sql, &[&pid]).await.map(|r| r.get(0)).map_err(|e| db_message(&e))
}

/// Tests that need a real server, see `query::server_tests`.
#[cfg(test)]
mod server_tests {
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
