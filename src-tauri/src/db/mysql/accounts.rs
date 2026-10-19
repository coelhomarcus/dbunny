//! MySQL's own way to list accounts and sessions: the `mysql` schema and the performance schema.

use mysql_async::prelude::Queryable;
use mysql_async::Row;

use crate::db::mysql_common::admin::{global_privileges, RawSession};
use crate::db::mysql_common::catalog::{number, string, text};
use crate::db::model::RoleInfo;

/// The accounts MySQL makes for its own use.
const BUILTIN_ACCOUNTS: [&str; 3] = ["mysql.sys", "mysql.session", "mysql.infoschema"];

/// The sessions with their transactions and applications: the performance schema and
/// `INNODB_TRX`. `database` limits them to those using it.
pub(super) async fn rich_sessions(conn: &mut mysql_async::Conn, database: Option<&str>) -> mysql_async::Result<Vec<RawSession>> {
    let filter = if database.is_some() { "WHERE p.DB = ?" } else { "" };
    let sql = format!(
        "SELECT p.ID AS id, p.USER AS user_name, p.HOST AS host, p.DB AS db_name, p.COMMAND AS command, \
                p.TIME AS seconds, p.STATE AS state, p.INFO AS info, \
                TIMESTAMPDIFF(SECOND, trx.trx_started, NOW()) AS trx_seconds, \
                COALESCE(program.ATTR_VALUE, client.ATTR_VALUE) AS application \
         FROM performance_schema.processlist p \
         LEFT JOIN information_schema.INNODB_TRX trx ON trx.trx_mysql_thread_id = p.ID \
         LEFT JOIN performance_schema.session_connect_attrs program ON program.PROCESSLIST_ID = p.ID AND program.ATTR_NAME = 'program_name' \
         LEFT JOIN performance_schema.session_connect_attrs client ON client.PROCESSLIST_ID = p.ID AND client.ATTR_NAME = '_client_name' \
         {filter} ORDER BY p.ID"
    );
    let rows: Vec<Row> = match database {
        Some(database) => conn.exec(sql, (database,)).await?,
        None => conn.query(sql).await?,
    };
    Ok(rows
        .iter()
        .map(|row| RawSession {
            id: number(row, "id").unwrap_or(0),
            user: text(row, "user_name"),
            host: text(row, "host"),
            database: text(row, "db_name"),
            command: string(row, "command"),
            seconds: text(row, "seconds").and_then(|s| s.parse().ok()),
            state: text(row, "state"),
            query: text(row, "info"),
            transaction_seconds: number(row, "trx_seconds").map(|s| s as f64),
            application: text(row, "application"),
        })
        .collect())
}

/// The accounts of the server. An account that is locked, or that another is granted as a role,
/// can't log in: it is a role. The server's own accounts are the built-in ones.
pub(super) async fn accounts(conn: &mut mysql_async::Conn) -> mysql_async::Result<Vec<RoleInfo>> {
    let rows: Vec<Row> = conn
        .query(
            "SELECT u.User AS name, u.Host AS host, u.account_locked AS locked, u.password_expired AS expired, \
                    u.Super_priv AS super_priv, u.Create_priv AS create_priv, u.Create_user_priv AS create_user_priv, \
                    u.Repl_slave_priv AS repl_priv, u.max_user_connections AS max_connections, \
                    CONCAT(u.User, '@', u.Host) = CURRENT_USER() AS is_current, \
                    (SELECT GROUP_CONCAT(IF(e.FROM_HOST = '%', e.FROM_USER, CONCAT(e.FROM_USER, '@', e.FROM_HOST)) ORDER BY e.FROM_USER SEPARATOR ',') \
                       FROM mysql.role_edges e WHERE e.TO_USER = u.User AND e.TO_HOST = u.Host) AS member_of, \
                    EXISTS (SELECT 1 FROM mysql.role_edges e WHERE e.FROM_USER = u.User AND e.FROM_HOST = u.Host) AS is_role \
             FROM mysql.user u",
        )
        .await?;
    let mut accounts: Vec<RoleInfo> = rows
        .iter()
        .map(|row| {
            let name = string(row, "name");
            let can_login = string(row, "locked") != "Y" && number(row, "is_role") != Some(1);
            let limit = number(row, "max_connections").unwrap_or(0);
            RoleInfo {
                is_builtin: BUILTIN_ACCOUNTS.contains(&name.as_str()),
                name,
                host: text(row, "host"),
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

/// Only the account of this session, for one that can't read the `mysql` schema: its global
/// privileges come from its own grants.
pub(super) async fn own_account(conn: &mut mysql_async::Conn) -> mysql_async::Result<RoleInfo> {
    let current: String = conn.query_first("SELECT CURRENT_USER()").await?.unwrap_or_default();
    let (name, host) = current.rsplit_once('@').map_or((current.clone(), None), |(name, host)| (name.to_string(), Some(host.to_string())));
    let grants: Vec<String> = conn.query("SHOW GRANTS").await?;
    let privileges = global_privileges(&grants);
    Ok(RoleInfo {
        name,
        host,
        can_login: true,
        superuser: privileges.superuser,
        create_db: privileges.create_db,
        create_role: privileges.create_role,
        replication: privileges.replication,
        bypass_rls: None,
        connection_limit: -1,
        valid_until: None,
        member_of: vec![],
        is_current: true,
        is_builtin: false,
    })
}

/// Who waits on whom, as `(waiting, blocking)` process ids. Without the permission to see it
/// nobody is shown as blocked.
pub(super) async fn blockers(conn: &mut mysql_async::Conn) -> Vec<(i64, i64)> {
    let waits: Vec<(Option<i64>, Option<i64>)> = conn
        .query(
            "SELECT DISTINCT waiting.PROCESSLIST_ID, blocking.PROCESSLIST_ID FROM performance_schema.data_lock_waits w \
             JOIN performance_schema.threads waiting ON waiting.THREAD_ID = w.REQUESTING_THREAD_ID \
             JOIN performance_schema.threads blocking ON blocking.THREAD_ID = w.BLOCKING_THREAD_ID",
        )
        .await
        .unwrap_or_default();
    waits.into_iter().filter_map(|(waiting, blocking)| Some((waiting?, blocking?))).collect()
}

/// The ids of the other sessions that use database `name`.
pub(super) async fn session_ids_in(conn: &mut mysql_async::Conn, name: &str) -> mysql_async::Result<Vec<i64>> {
    conn.exec("SELECT ID FROM performance_schema.processlist WHERE DB = ? AND ID <> CONNECTION_ID()", (name,)).await
}
