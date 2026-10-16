//! The server's roles, for the Roles page.

use tauri::State;
use tokio_postgres::Client;

use super::query::db_message;
use super::state::AppState;
use crate::db::model::RoleInfo;

#[tauri::command]
pub async fn get_roles(state: State<'_, AppState>, session_id: String) -> Result<Vec<RoleInfo>, String> {
    let client = state.postgres(&session_id).await?.client().await?;
    roles(&client).await.map_err(|e| db_message(&e))
}

pub(crate) async fn roles(client: &Client) -> Result<Vec<RoleInfo>, tokio_postgres::Error> {
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

/// Tests that need a real server, see `query::server_tests`.
#[cfg(test)]
mod server_tests {
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
