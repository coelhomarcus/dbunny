//! MariaDB's way to list accounts and sessions: its roles are accounts of their own, and
//! with the performance schema off by default the process list comes from `information_schema`.

use mysql_async::prelude::Queryable;
use mysql_async::Row;

use crate::db::mysql_common::admin::{raw_sessions, RawSession};
use crate::db::mysql_common::catalog::{number, string, text};
use crate::db::model::RoleInfo;

/// The accounts MariaDB makes for its own use.
const BUILTIN_ACCOUNTS: [&str; 1] = ["mariadb.sys"];

/// The accounts and roles of the server. A role, or an account that is locked, can't log in.
/// Whether an account is locked is kept in the JSON of `mysql.global_priv`.
pub(super) async fn accounts(conn: &mut mysql_async::Conn) -> mysql_async::Result<Vec<RoleInfo>> {
    let rows: Vec<Row> = conn
        .query(
            "SELECT u.User AS name, u.Host AS host, \
                    COALESCE(JSON_VALUE(g.Priv, '$.account_locked'), '0') IN ('1', 'true') AS locked, u.password_expired AS expired, \
                    u.Super_priv AS super_priv, u.Create_priv AS create_priv, u.Create_user_priv AS create_user_priv, \
                    u.Repl_slave_priv AS repl_priv, u.max_user_connections AS max_connections, \
                    CONCAT(u.User, '@', u.Host) = CURRENT_USER() AS is_current, \
                    (SELECT GROUP_CONCAT(m.Role ORDER BY m.Role SEPARATOR ',') FROM mysql.roles_mapping m \
                      WHERE m.User = u.User AND m.Host = u.Host) AS member_of, \
                    u.is_role = 'Y' AS is_role \
             FROM mysql.user u LEFT JOIN mysql.global_priv g ON g.User = u.User AND g.Host = u.Host",
        )
        .await?;
    let mut accounts: Vec<RoleInfo> = rows
        .iter()
        .map(|row| {
            let name = string(row, "name");
            let can_login = number(row, "locked") != Some(1) && number(row, "is_role") != Some(1);
            let limit = number(row, "max_connections").unwrap_or(0);
            RoleInfo {
                is_builtin: BUILTIN_ACCOUNTS.contains(&name.as_str()),
                name,
                // A role has no host
                host: text(row, "host").filter(|host| !host.is_empty()),
                can_login,
                superuser: string(row, "super_priv") == "Y",
                create_db: string(row, "create_priv") == "Y",
                create_role: string(row, "create_user_priv") == "Y",
                replication: string(row, "repl_priv") == "Y",
                bypass_rls: None,
                connection_limit: if limit > 0 { limit as i32 } else { -1 },
                valid_until: (can_login && string(row, "expired") == "Y").then(|| "expired".to_string()),
                member_of: text(row, "member_of").map(|names| names.split(',').map(String::from).collect()).unwrap_or_default(),
                is_current: number(row, "is_current") == Some(1),
            }
        })
        .collect();
    accounts.sort_by(|a, b| (a.is_builtin, !a.can_login, &a.name, &a.host).cmp(&(b.is_builtin, !b.can_login, &b.name, &b.host)));
    Ok(accounts)
}

/// The sessions with the transaction each holds open, from `information_schema`. There are no
/// connection attributes without the performance schema, so no application is known.
pub(super) async fn rich_sessions(conn: &mut mysql_async::Conn, database: Option<&str>) -> mysql_async::Result<Vec<RawSession>> {
    let filter = if database.is_some() { "WHERE p.DB = ?" } else { "" };
    let sql = format!(
        "SELECT p.ID AS id, p.USER AS user_name, p.HOST AS host, p.DB AS db_name, p.COMMAND AS command, \
                p.TIME AS seconds, p.STATE AS state, p.INFO AS info, \
                TIMESTAMPDIFF(SECOND, trx.trx_started, NOW()) AS trx_seconds, NULL AS application \
         FROM information_schema.PROCESSLIST p \
         LEFT JOIN information_schema.INNODB_TRX trx ON trx.trx_mysql_thread_id = p.ID \
         {filter} ORDER BY p.ID"
    );
    let rows: Vec<Row> = match database {
        Some(database) => conn.exec(sql, (database,)).await?,
        None => conn.query(sql).await?,
    };
    Ok(raw_sessions(&rows))
}

/// Who waits on whom, as `(waiting, blocking)` process ids. Without the permission to see it
/// nobody is shown as blocked.
pub(super) async fn blockers(conn: &mut mysql_async::Conn) -> Vec<(i64, i64)> {
    let waits: Vec<(Option<i64>, Option<i64>)> = conn
        .query(
            "SELECT DISTINCT waiting.trx_mysql_thread_id, blocking.trx_mysql_thread_id FROM information_schema.INNODB_LOCK_WAITS w \
             JOIN information_schema.INNODB_TRX waiting ON waiting.trx_id = w.requesting_trx_id \
             JOIN information_schema.INNODB_TRX blocking ON blocking.trx_id = w.blocking_trx_id",
        )
        .await
        .unwrap_or_default();
    waits.into_iter().filter_map(|(waiting, blocking)| Some((waiting?, blocking?))).collect()
}

/// The ids of the other sessions that use database `name`.
pub(super) async fn session_ids_in(conn: &mut mysql_async::Conn, name: &str) -> mysql_async::Result<Vec<i64>> {
    conn.exec("SELECT ID FROM information_schema.PROCESSLIST WHERE DB = ? AND ID <> CONNECTION_ID()", (name,)).await
}
