use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use crate::db::net::ssh::SshConfig;
use crate::db::Engine;
use super::state::AppState;
use super::storage::{save_connections_to_file, ConnectionDetails, SavedConnection, SavedConnectionInfo, TlsSettings};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnectionInput {
    pub id: Option<String>,
    pub name: String,
    /// Missing from frontends that only knew PostgreSQL.
    #[serde(default)]
    pub engine: Engine,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    #[serde(default)]
    pub ssh: Option<SshConfig>,
    #[serde(default)]
    pub tls: Option<TlsSettings>,
    pub ssl: bool,
    /// The database file of a SQLite connection; missing from frontends that only knew PostgreSQL.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub read_only: bool,
    pub color: String,
    #[serde(default)]
    pub icon: String,
}

#[tauri::command]
pub async fn get_saved_connections(
    state: State<'_, AppState>,
) -> Result<Vec<SavedConnectionInfo>, String> {
    let connections = state.saved_connections.lock().await;

    Ok(connections
        .iter()
        .filter_map(|c| c.to_info(&state.encryption_key).ok())
        .collect())
}

impl SavedConnectionInput {
    /// What is saved for its engine; the fields of the other engines are ignored.
    fn into_details(self) -> Result<ConnectionDetails, String> {
        match self.engine {
            Engine::Postgres => Ok(ConnectionDetails::Postgres {
                host: self.host,
                port: self.port,
                database: self.database,
                user: self.user,
                password: self.password,
                ssh: self.ssh,
                tls: self.tls,
                ssl: self.ssl,
            }),
            Engine::Sqlite => {
                if self.path.trim().is_empty() {
                    return Err("path is required".into());
                }
                Ok(ConnectionDetails::Sqlite { path: self.path })
            }
        }
    }
}

/// Adds `input` to `connections`, or replaces the one whose id it carries (keeping its id
/// and creation time). Returns the connection as saved; `connections` is untouched on error.
fn upsert(
    connections: &mut Vec<SavedConnection>,
    key: &[u8; 32],
    input: SavedConnectionInput,
    now: String,
) -> Result<SavedConnection, String> {
    let pos = match input.id {
        Some(ref id) => Some(
            connections
                .iter()
                .position(|c| &c.id == id)
                .ok_or_else(|| format!("Connection with id {} not found", id))?,
        ),
        None => None,
    };
    let (id, created_at) = match pos {
        Some(pos) => (connections[pos].id.clone(), connections[pos].created_at.clone()),
        None => (Uuid::new_v4().to_string(), now.clone()),
    };

    let (name, color, read_only, icon) = (input.name.clone(), input.color.clone(), input.read_only, input.icon.clone());
    let mut saved = SavedConnection::new(key, id, name, input.into_details()?, color, created_at, now)?;
    saved.read_only = read_only;
    saved.icon = icon;

    match pos {
        Some(pos) => connections[pos] = saved.clone(),
        None => connections.push(saved.clone()),
    }
    Ok(saved)
}

#[tauri::command]
pub async fn save_connection(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    connection: SavedConnectionInput,
) -> Result<SavedConnectionInfo, String> {
    let mut connections = state.saved_connections.lock().await;
    // Restored if the file can't be written, so memory and disk don't disagree
    let previous = connections.clone();
    let now = chrono::Utc::now().to_rfc3339();

    let saved = upsert(&mut connections, &state.encryption_key, connection, now)?;

    if let Err(e) = save_connections_to_file(&app_handle, &connections) {
        *connections = previous;
        return Err(format!("The connection wasn't saved: {e}"));
    }

    saved.to_info(&state.encryption_key)
}

#[tauri::command]
pub async fn delete_saved_connection(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    let mut connections = state.saved_connections.lock().await;
    let previous = connections.clone();
    connections.retain(|c| c.id != id);

    if connections.len() == previous.len() {
        return Err(format!("Connection with id {} not found", id));
    }

    if let Err(e) = save_connections_to_file(&app_handle, &connections) {
        *connections = previous;
        return Err(format!("The connection wasn't deleted: {e}"));
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [3u8; 32];

    /// An input as the frontend sends it, with the PostgreSQL fields filled in.
    fn input(engine: &str, id: Option<&str>, path: &str) -> SavedConnectionInput {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": "Inventory", "engine": engine, "host": "db.example.com", "port": 5432, "database": "shop",
            "user": "me", "password": "secret", "ssh": null, "tls": { "mode": "require" }, "ssl": true, "path": path,
            "readOnly": true, "color": "#22c55e", "icon": "box"
        }))
        .unwrap()
    }

    #[test]
    fn a_sqlite_connection_without_a_path_isnt_saved() {
        let mut connections = Vec::new();
        let err = upsert(&mut connections, &KEY, input("sqlite", None, ""), "now".into()).err().unwrap();
        assert_eq!(err, "path is required");
        assert!(connections.is_empty());
        // Blank space isn't a path either
        let err = upsert(&mut connections, &KEY, input("sqlite", None, "  \t "), "now".into()).err().unwrap();
        assert_eq!(err, "path is required");
        assert!(connections.is_empty());

        // Nor is an edit that empties it
        upsert(&mut connections, &KEY, input("sqlite", None, "/data/a.sqlite"), "t1".into()).unwrap();
        let id = connections[0].id.clone();
        let err = upsert(&mut connections, &KEY, input("sqlite", Some(&id), ""), "t2".into()).err().unwrap();
        assert_eq!(err, "path is required");
        assert_eq!(connections[0].to_info(&KEY).unwrap().path, "/data/a.sqlite");
    }

    #[test]
    fn a_sqlite_connection_saves_only_its_path() {
        let mut connections = Vec::new();
        let saved = upsert(&mut connections, &KEY, input("sqlite", None, "/data/a.sqlite"), "t1".into()).unwrap();
        assert_eq!(connections.len(), 1);
        assert!(!saved.encrypted_data.contains("/data") && !saved.ssl && saved.read_only);
        let info = saved.to_info(&KEY).unwrap();
        assert_eq!((info.engine, info.path.as_str()), (Engine::Sqlite, "/data/a.sqlite"));
        assert_eq!(
            (info.host.as_str(), info.port, info.database.as_str(), info.user.as_str(), info.password.as_str()),
            ("", 0, "", "", "")
        );
        assert!(info.ssh.is_none() && info.tls.is_none() && !info.ssl);
        assert_eq!((info.name.as_str(), info.color.as_str(), info.icon.as_str(), info.read_only), ("Inventory", "#22c55e", "box", true));
        assert_eq!((info.created_at.as_str(), info.updated_at.as_str()), ("t1", "t1"));

        // An edit keeps the id and the creation time
        let id = info.id.clone();
        let edited = upsert(&mut connections, &KEY, input("sqlite", Some(&id), "/data/b.sqlite"), "t2".into()).unwrap();
        assert_eq!(connections.len(), 1);
        let info = edited.to_info(&KEY).unwrap();
        assert_eq!((info.id.as_str(), info.path.as_str()), (id.as_str(), "/data/b.sqlite"));
        assert_eq!((info.created_at.as_str(), info.updated_at.as_str()), ("t1", "t2"));
        assert_eq!(connections[0].to_info(&KEY).unwrap().path, "/data/b.sqlite");
    }

    #[test]
    fn a_postgres_connection_saves_as_before_and_ignores_the_path() {
        let mut connections = Vec::new();
        let saved = upsert(&mut connections, &KEY, input("postgres", None, "/data/a.sqlite"), "t1".into()).unwrap();
        let info = saved.to_info(&KEY).unwrap();
        assert_eq!((info.engine, info.path.as_str(), info.ssl, info.read_only), (Engine::Postgres, "", true, true));
        assert_eq!(
            (info.host.as_str(), info.port, info.database.as_str(), info.user.as_str(), info.password.as_str()),
            ("db.example.com", 5432, "shop", "me", "secret")
        );
        assert_eq!(info.tls.map(|t| t.mode), Some("require".to_string()));

        // From a frontend that sends neither engine nor path
        let old: SavedConnectionInput = serde_json::from_value(serde_json::json!({
            "id": info.id, "name": "Prod", "host": "h", "port": 5433, "database": "d", "user": "u", "password": "p",
            "ssl": false, "color": "#000"
        }))
        .unwrap();
        let edited = upsert(&mut connections, &KEY, old, "t2".into()).unwrap().to_info(&KEY).unwrap();
        assert_eq!((edited.id, edited.engine, edited.host.as_str(), edited.port, edited.path.as_str()), (info.id, Engine::Postgres, "h", 5433, ""));
        assert_eq!((edited.created_at.as_str(), edited.updated_at.as_str(), edited.read_only), ("t1", "t2", false));
    }

    #[test]
    fn editing_a_missing_connection_fails() {
        let mut connections = Vec::new();
        let err = upsert(&mut connections, &KEY, input("sqlite", Some("nope"), "/data/a.sqlite"), "t".into()).err().unwrap();
        assert_eq!(err, "Connection with id nope not found");
        assert!(connections.is_empty());
    }
}
