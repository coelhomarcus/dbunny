use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

use crate::db::net::ssh::SshConfig;
use crate::db::Engine;

const PBKDF2_SALT: &[u8] = b"dbunny-connection-salt-v1";
const PBKDF2_ITERATIONS: u32 = 100_000;

/// Sensitive data of a PostgreSQL connection — encrypted at rest as a JSON blob.
#[derive(Serialize, Deserialize)]
struct SensitiveData {
    host: String,
    port: u16,
    database: String,
    user: String,
    password: String,
    /// SSH tunnel settings, secrets included; absent in files from older versions.
    #[serde(default)]
    ssh: Option<SshConfig>,
    /// How SSL is verified and which certificate files to use; absent in older files.
    #[serde(default)]
    tls: Option<TlsSettings>,
}

/// Sensitive data of a SQLite connection — encrypted at rest as a JSON blob, like the
/// PostgreSQL one: the file's path says as much about the user as a host does.
#[derive(Serialize, Deserialize)]
struct SqliteData {
    path: String,
}

pub use crate::db::net::tls::TlsSettings;

fn default_engine() -> String {
    Engine::default().as_str().to_string()
}

/// Stored on disk. Only metadata is plaintext; everything else is in `encrypted_data`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    /// Which database it is, and so what `encrypted_data` holds; missing in files from
    /// older versions, which only knew PostgreSQL. Kept as the stored text: a connection of an
    /// engine this version doesn't know isn't offered, but it stays in the file untouched
    /// (see `engine()`).
    #[serde(default = "default_engine")]
    pub engine: String,
    pub encrypted_data: String,
    pub ssl: bool,
    /// Every transaction starts read-only; missing in files from older versions.
    #[serde(default)]
    pub read_only: bool,
    pub color: String,
    /// Name of the icon picked for it; empty (and missing in older files) for the default.
    #[serde(default)]
    pub icon: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Returned to the frontend with all fields decrypted.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnectionInfo {
    pub id: String,
    pub name: String,
    pub engine: Engine,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    pub ssh: Option<SshConfig>,
    pub tls: Option<TlsSettings>,
    /// The database file of a SQLite connection; empty for the other engines.
    pub path: String,
    pub ssl: bool,
    pub read_only: bool,
    pub color: String,
    pub icon: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize)]
pub struct ConnectionsFile {
    pub version: u32,
    pub connections: Vec<SavedConnection>,
}

// ── Key management ────────────────────────────────────────────────────────────

/// The key older versions used: derived from the machine's hostname, so it changed (and
/// the saved connections became unreadable) whenever the hostname did. Only kept to read
/// those files once and move them to the app's own key.
pub fn hostname_key() -> [u8; 32] {
    use pbkdf2::pbkdf2_hmac;
    use sha2::Sha256;

    let machine_name = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "dbunny-default-host".to_string());

    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(machine_name.as_bytes(), PBKDF2_SALT, PBKDF2_ITERATIONS, &mut key);
    key
}

const KEY_FILE: &str = "encryption.key";

/// The app's own random key, kept in `dir`; made on first use. A key that is there but
/// can't be read is an error rather than a reason to make a new one, which would lock
/// out every connection it encrypted.
pub fn load_or_create_key(dir: &Path) -> Result<[u8; 32], String> {
    let path = dir.join(KEY_FILE);
    match fs::read(&path) {
        Ok(bytes) => {
            if let Ok(key) = <[u8; 32]>::try_from(bytes.as_slice()) {
                return Ok(key);
            }
            // A damaged key can't be repaired; it's set aside, not overwritten
            fs::rename(&path, aside(&path, "invalid")).map_err(|e| format!("Couldn't move the damaged key aside: {e}"))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Couldn't read the encryption key: {e}")),
    }

    fs::create_dir_all(dir).map_err(|e| format!("Failed to create app data dir: {e}"))?;
    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    write_private(&path, &key)?;
    Ok(key)
}

/// Writes a file only the user can read, through a temporary file so it's never half-written.
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let temp = path.with_extension("tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&temp).map_err(|e| format!("Couldn't write the encryption key: {e}"))?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| format!("Couldn't write the encryption key: {e}"))?;
    fs::rename(&temp, path).map_err(|e| format!("Couldn't write the encryption key: {e}"))
}

/// Re-encrypts with `key` the connections only `legacy` can open. Returns whether any changed.
pub fn migrate_to_key(connections: &mut [SavedConnection], key: &[u8; 32], legacy: &[u8; 32]) -> bool {
    if key == legacy {
        return false;
    }
    let mut changed = false;
    for conn in connections.iter_mut() {
        if decrypt(key, &conn.encrypted_data).is_ok() {
            continue;
        }
        if let Some(data) = decrypt(legacy, &conn.encrypted_data).ok().and_then(|plain| encrypt(key, &plain).ok()) {
            conn.encrypted_data = data;
            changed = true;
        }
    }
    changed
}

/// `<file>.<label>-<timestamp>`, next to the file: where an unusable file is kept.
fn aside(path: &Path, label: &str) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{name}.{label}-{}", chrono::Utc::now().format("%Y%m%d%H%M%S")))
}

// ── Crypto helpers ────────────────────────────────────────────────────────────

pub fn encrypt(key: &[u8; 32], plaintext: &str) -> Result<String, String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| format!("Failed to create cipher: {e}"))?;

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {e}"))?;

    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(BASE64.encode(&combined))
}

pub fn decrypt(key: &[u8; 32], encrypted: &str) -> Result<String, String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| format!("Failed to create cipher: {e}"))?;

    let combined = BASE64
        .decode(encrypted)
        .map_err(|e| format!("Base64 decode failed: {e}"))?;

    if combined.len() < 13 {
        return Err("Invalid encrypted data".into());
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {e}"))?;

    String::from_utf8(plaintext).map_err(|e| format!("Invalid UTF-8: {e}"))
}

// ── Storage helpers ───────────────────────────────────────────────────────────

pub fn get_storage_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {e}"))?;

    Ok(app_data_dir.join("db_connections.json"))
}

/// The saved connections. A file that exists but can't be read is moved aside first, so
/// the next save starts a new file instead of overwriting the connections in it.
pub fn load_connections_from_path(path: &Path) -> Vec<SavedConnection> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            eprintln!("Couldn't read {}: {e}", path.display());
            let _ = fs::rename(path, aside(path, "unreadable"));
            return Vec::new();
        }
    };
    match serde_json::from_str::<ConnectionsFile>(&content) {
        Ok(file) => file.connections,
        Err(e) => {
            eprintln!("{} isn't a connections file ({e}); keeping it aside", path.display());
            let _ = fs::rename(path, aside(path, "unreadable"));
            Vec::new()
        }
    }
}

pub fn save_connections_to_file(
    app_handle: &tauri::AppHandle,
    connections: &[SavedConnection],
) -> Result<(), String> {
    write_connections(&get_storage_path(app_handle)?, connections)
}

pub fn write_connections(path: &Path, connections: &[SavedConnection]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create app data dir: {e}"))?;
    }

    let file = ConnectionsFile {
        version: 2,
        connections: connections.to_vec(),
    };

    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("Failed to serialize connections: {e}"))?;

    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, &json)
        .map_err(|e| format!("Failed to write temp file: {e}"))?;
    fs::rename(&temp_path, path)
        .map_err(|e| format!("Failed to rename temp file: {e}"))?;

    Ok(())
}

// ── SavedConnection impl ──────────────────────────────────────────────────────

/// What a connection is saved with, by engine: the details that go into `encrypted_data`.
pub enum ConnectionDetails {
    Postgres {
        host: String,
        port: u16,
        database: String,
        user: String,
        password: String,
        ssh: Option<SshConfig>,
        tls: Option<TlsSettings>,
        ssl: bool,
    },
    Sqlite {
        path: String,
    },
}

impl ConnectionDetails {
    pub fn engine(&self) -> Engine {
        match self {
            ConnectionDetails::Postgres { .. } => Engine::Postgres,
            ConnectionDetails::Sqlite { .. } => Engine::Sqlite,
        }
    }
}

impl SavedConnection {
    pub fn new(
        key: &[u8; 32],
        id: String,
        name: String,
        details: ConnectionDetails,
        color: String,
        created_at: String,
        updated_at: String,
    ) -> Result<Self, String> {
        let engine = details.engine();
        // What gets encrypted depends on the engine
        let (json, ssl) = match details {
            ConnectionDetails::Postgres { host, port, database, user, password, ssh, tls, ssl } => {
                let json = serde_json::to_string(&SensitiveData { host, port, database, user, password, ssh, tls })
                    .map_err(|e| format!("Failed to serialize sensitive data: {e}"))?;
                (json, ssl)
            }
            // A SQLite file has no SSL
            ConnectionDetails::Sqlite { path } => {
                let json = serde_json::to_string(&SqliteData { path })
                    .map_err(|e| format!("Failed to serialize sensitive data: {e}"))?;
                (json, false)
            }
        };
        let encrypted_data = encrypt(key, &json)?;

        Ok(Self {
            id,
            name,
            engine: engine.as_str().to_string(),
            encrypted_data,
            ssl,
            read_only: false,
            color,
            icon: String::new(),
            created_at,
            updated_at,
        })
    }

    /// Its engine, or None when it was saved by a newer version for an engine this one doesn't know.
    pub fn engine(&self) -> Option<Engine> {
        Engine::from_name(&self.engine)
    }

    pub fn to_info(&self, key: &[u8; 32]) -> Result<SavedConnectionInfo, String> {
        let engine = self
            .engine()
            .ok_or_else(|| format!("This version doesn't support {} connections", self.engine))?;
        let json = decrypt(key, &self.encrypted_data)?;
        let deserialize_error = |e: serde_json::Error| format!("Failed to deserialize sensitive data: {e}");
        // An entry without an engine reads as PostgreSQL, but an older version that doesn't know the
        // engine field drops it when it saves the file: one that holds a SQLite path is still SQLite
        let engine = match engine {
            Engine::Postgres if serde_json::from_str::<SensitiveData>(&json).is_err() && serde_json::from_str::<SqliteData>(&json).is_ok() => {
                Engine::Sqlite
            }
            other => other,
        };
        // A SQLite connection has only its path; the PostgreSQL fields are left empty
        let (sensitive, path) = match engine {
            Engine::Postgres => (serde_json::from_str::<SensitiveData>(&json).map_err(deserialize_error)?, String::new()),
            Engine::Sqlite => {
                let SqliteData { path } = serde_json::from_str(&json).map_err(deserialize_error)?;
                let empty = SensitiveData {
                    host: String::new(),
                    port: 0,
                    database: String::new(),
                    user: String::new(),
                    password: String::new(),
                    ssh: None,
                    tls: None,
                };
                (empty, path)
            }
        };

        Ok(SavedConnectionInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            engine,
            host: sensitive.host,
            port: sensitive.port,
            database: sensitive.database,
            user: sensitive.user,
            password: sensitive.password,
            ssh: sensitive.ssh,
            tls: sensitive.tls,
            path,
            ssl: self.ssl,
            read_only: self.read_only,
            color: self.color.clone(),
            icon: self.icon.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_from_older_versions_still_load() {
        // Written before read_only and icon existed
        let old = r##"{"id":"1","name":"Prod","encryptedData":"x","ssl":true,"color":"#ef4444","createdAt":"a","updatedAt":"b"}"##;
        let conn: SavedConnection = serde_json::from_str(old).unwrap();
        assert!(!conn.read_only);
        assert_eq!(conn.icon, "", "no icon means the default one");
    }

    #[test]
    fn the_icon_is_kept_next_to_the_encrypted_details() {
        let key = [7u8; 32];
        let mut conn = SavedConnection::new(
            &key, "1".into(), "Prod".into(), postgres("db.example.com", 5432, "shop", "me", "secret", true), "#ef4444".into(), "a".into(), "b".into(),
        )
        .unwrap();
        conn.icon = "rocket".into();
        let stored = serde_json::to_string(&conn).unwrap();
        assert!(stored.contains(r#""icon":"rocket""#) && !stored.contains("secret"), "{stored}");
        let info = serde_json::from_str::<SavedConnection>(&stored).unwrap().to_info(&key).unwrap();
        assert_eq!((info.icon.as_str(), info.host.as_str()), ("rocket", "db.example.com"));
    }

    /// An encrypted payload exactly as versions before the engine field wrote it.
    fn legacy_payload(key: &[u8; 32]) -> String {
        let sensitive = r#"{"host":"db.example.com","port":5433,"database":"shop","user":"me","password":"secret","ssh":{"host":"bastion","port":2222,"user":"ubuntu","authMethod":"key","password":"","keyPath":"/home/me/.ssh/id_ed25519","passphrase":"pp"},"tls":{"mode":"verify-full","rootCert":"/ca.pem","cert":"/client.pem","key":"/client.key"}}"#;
        encrypt(key, sensitive).unwrap()
    }

    #[test]
    fn a_sqlite_entry_stripped_of_its_engine_by_an_older_version_is_still_found() {
        // 0.4.0 doesn't know the engine field: saving rewrites every entry without it
        let key = [7u8; 32];
        let payload = encrypt(&key, r#"{"path":"/Users/me/notes.sqlite"}"#).unwrap();
        let json = format!(
            r##"{{"version":1,"connections":[{{"id":"s","name":"Notes","encryptedData":"{payload}","ssl":false,"color":"#000","createdAt":"t","updatedAt":"t"}}]}}"##
        );
        let file: ConnectionsFile = serde_json::from_str(&json).unwrap();
        let info = file.connections[0].to_info(&key).unwrap();
        assert_eq!((info.engine, info.path.as_str()), (Engine::Sqlite, "/Users/me/notes.sqlite"));
        assert_eq!((info.host.as_str(), info.port), ("", 0));

        // A payload that is neither kind is still an error, not a guess
        let junk = encrypt(&key, r#"{"x":1}"#).unwrap();
        let mut broken = file.connections[0].clone();
        broken.encrypted_data = junk;
        assert!(broken.to_info(&key).is_err());
    }

    #[test]
    fn files_without_engine_load_as_postgres() {
        let key = [7u8; 32];
        let json = format!(
            r##"{{"version":1,"connections":[{{"id":"a","name":"x","encryptedData":"{}","ssl":false,"color":"#000","createdAt":"t","updatedAt":"t"}}]}}"##,
            legacy_payload(&key)
        );
        let file: ConnectionsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(file.connections[0].engine(), Some(Engine::Postgres));
        assert_eq!(file.connections[0].to_info(&key).unwrap().engine, Engine::Postgres);
    }

    #[test]
    fn engine_is_written_back() {
        let key = [7u8; 32];
        let conn = SavedConnection::new(
            &key, "1".into(), "Prod".into(), postgres("db.example.com", 5432, "shop", "me", "secret", true), "#ef4444".into(), "a".into(), "b".into(),
        )
        .unwrap();
        let json = serde_json::to_value(&conn).unwrap();
        assert_eq!(json["engine"], "postgres");
        let info = serde_json::to_value(conn.to_info(&key).unwrap()).unwrap();
        assert_eq!(info["engine"], "postgres");
    }

    #[test]
    fn a_file_from_the_previous_version_loads_and_is_written_back_with_its_engine() {
        let key = [9u8; 32];
        let path = std::env::temp_dir().join(format!("dbunny-legacy-connections-{}.json", uuid::Uuid::new_v4()));
        let legacy = format!(
            r##"{{"version":1,"connections":[{{"id":"a","name":"Prod","encryptedData":"{}","ssl":true,"readOnly":true,"color":"#ef4444","icon":"rocket","createdAt":"t1","updatedAt":"t2"}}]}}"##,
            legacy_payload(&key)
        );
        fs::write(&path, &legacy).unwrap();

        let check = |connections: &[SavedConnection]| {
            assert_eq!(connections.len(), 1);
            assert_eq!(connections[0].engine(), Some(Engine::Postgres));
            let info = connections[0].to_info(&key).unwrap();
            assert_eq!(info.engine, Engine::Postgres);
            assert_eq!(
                (info.host.as_str(), info.port, info.database.as_str(), info.user.as_str(), info.password.as_str()),
                ("db.example.com", 5433, "shop", "me", "secret")
            );
            let ssh = info.ssh.expect("the ssh settings");
            assert_eq!(
                (ssh.host.as_str(), ssh.port, ssh.user.as_str(), ssh.auth_method.as_str(), ssh.key_path.as_str(), ssh.passphrase.as_str()),
                ("bastion", 2222, "ubuntu", "key", "/home/me/.ssh/id_ed25519", "pp")
            );
            let tls = info.tls.expect("the tls settings");
            assert_eq!(
                (tls.mode.as_str(), tls.root_cert.as_str(), tls.cert.as_str(), tls.key.as_str()),
                ("verify-full", "/ca.pem", "/client.pem", "/client.key")
            );
            assert!(info.ssl && info.read_only);
            assert_eq!((info.name.as_str(), info.icon.as_str(), info.created_at.as_str(), info.updated_at.as_str()), ("Prod", "rocket", "t1", "t2"));
        };

        let loaded = load_connections_from_path(&path);
        check(&loaded);

        write_connections(&path, &loaded).unwrap();
        let written: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["connections"][0]["engine"], "postgres");
        check(&load_connections_from_path(&path));

        let _ = fs::remove_file(&path);
    }

    /// A connections file with a PostgreSQL entry and one a newer version wrote for an engine
    /// this build doesn't know.
    fn file_with_an_unknown_engine(key: &[u8; 32]) -> (String, String) {
        let mysql_payload = encrypt(key, r#"{"host":"my.example.com","port":3306,"user":"root","password":"pw"}"#).unwrap();
        let json = format!(
            r##"{{"version":2,"connections":[{{"id":"pg","name":"Prod","engine":"postgres","encryptedData":"{}","ssl":true,"color":"#ef4444","createdAt":"t1","updatedAt":"t2"}},{{"id":"my","name":"Shop","engine":"mysql","encryptedData":"{mysql_payload}","ssl":false,"readOnly":true,"color":"#3b82f6","icon":"cart","createdAt":"t3","updatedAt":"t4"}}]}}"##,
            legacy_payload(key)
        );
        (json, mysql_payload)
    }

    #[test]
    fn a_connection_of_an_unknown_engine_doesnt_make_the_file_unreadable() {
        let key = [5u8; 32];
        let dir = temp_dir();
        let path = dir.join("db_connections.json");
        fs::write(&path, file_with_an_unknown_engine(&key).0).unwrap();

        let loaded = load_connections_from_path(&path);
        assert!(path.exists(), "the file isn't moved aside");
        assert_eq!(loaded.len(), 2, "both connections are kept");
        // What get_saved_connections offers: only the engines this build knows
        let offered: Vec<_> = loaded.iter().filter_map(|c| c.to_info(&key).ok()).collect();
        assert_eq!(offered.len(), 1);
        assert_eq!((offered[0].id.as_str(), offered[0].engine), ("pg", Engine::Postgres));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_connection_of_an_unknown_engine_is_written_back_unchanged() {
        let key = [5u8; 32];
        let dir = temp_dir();
        let path = dir.join("db_connections.json");
        let (json, mysql_payload) = file_with_an_unknown_engine(&key);
        fs::write(&path, json).unwrap();
        let unknown = |path: &Path| {
            let written: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
            let entries = written["connections"].as_array().unwrap().clone();
            entries.into_iter().find(|c| c["id"] == "my").expect("the mysql connection")
        };
        let check = |entry: serde_json::Value| {
            assert_eq!(entry["engine"], "mysql");
            assert_eq!(entry["encryptedData"].as_str().unwrap(), mysql_payload);
            assert_eq!((entry["name"].as_str(), entry["icon"].as_str(), entry["readOnly"].as_bool()), (Some("Shop"), Some("cart"), Some(true)));
            assert_eq!((entry["createdAt"].as_str(), entry["updatedAt"].as_str()), (Some("t3"), Some("t4")));
        };

        // Saving the list as it is
        let loaded = load_connections_from_path(&path);
        write_connections(&path, &loaded).unwrap();
        check(unknown(&path));

        // Saving after another connection is added and the PostgreSQL one deleted
        let mut connections = load_connections_from_path(&path);
        assert_eq!(connections.len(), 2);
        connections.push(connection(&key, "new"));
        connections.retain(|c| c.id != "pg");
        write_connections(&path, &connections).unwrap();
        let reloaded = load_connections_from_path(&path);
        assert_eq!(reloaded.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["my", "new"]);
        check(unknown(&path));
        let _ = fs::remove_dir_all(dir);
    }

    /// PostgreSQL details without SSH or custom SSL settings.
    fn postgres(host: &str, port: u16, database: &str, user: &str, password: &str, ssl: bool) -> ConnectionDetails {
        ConnectionDetails::Postgres {
            host: host.into(),
            port,
            database: database.into(),
            user: user.into(),
            password: password.into(),
            ssh: None,
            tls: None,
            ssl,
        }
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dbunny-storage-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn connection(key: &[u8; 32], name: &str) -> SavedConnection {
        SavedConnection::new(
            key, name.into(), name.into(), postgres("db.example.com", 5432, "shop", "me", "secret", false), "#3b82f6".into(), "a".into(), "b".into(),
        )
        .unwrap()
    }

    #[test]
    fn the_key_is_made_once_kept_private_and_reused() {
        let dir = temp_dir();
        let first = load_or_create_key(&dir).unwrap();
        assert_eq!(load_or_create_key(&dir).unwrap(), first, "the same key comes back");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join(KEY_FILE)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_damaged_key_is_set_aside_not_overwritten() {
        let dir = temp_dir();
        fs::write(dir.join(KEY_FILE), b"short").unwrap();
        let key = load_or_create_key(&dir).unwrap();
        assert_eq!(fs::read(dir.join(KEY_FILE)).unwrap(), key.to_vec());
        let kept: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with("encryption.key.invalid-")).collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(fs::read(kept[0].path()).unwrap(), b"short");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn connections_under_the_hostname_key_move_to_the_new_key() {
        let (legacy, key) = ([1u8; 32], [2u8; 32]);
        let mut connections = vec![connection(&legacy, "old"), connection(&key, "new")];
        assert!(migrate_to_key(&mut connections, &key, &legacy));
        for conn in &connections {
            assert_eq!(conn.to_info(&key).unwrap().password, "secret");
        }
        assert!(!migrate_to_key(&mut connections, &key, &legacy), "nothing left to move");
    }

    #[test]
    fn an_unreadable_connections_file_is_kept_aside() {
        let dir = temp_dir();
        let path = dir.join("db_connections.json");
        fs::write(&path, "{ not json").unwrap();
        assert!(load_connections_from_path(&path).is_empty());
        assert!(!path.exists(), "a save now starts a new file");
        let kept: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with("db_connections.json.unreadable-")).collect();
        assert_eq!(fs::read_to_string(kept[0].path()).unwrap(), "{ not json");

        // A good file reads back what was written
        let key = [3u8; 32];
        write_connections(&path, &[connection(&key, "prod")]).unwrap();
        let loaded = load_connections_from_path(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].to_info(&key).unwrap().name, "prod");
        let _ = fs::remove_dir_all(dir);
    }

    const SQLITE_PATH: &str = "/Users/me/Projects/shop/inventory-private.sqlite";

    fn sqlite(key: &[u8; 32], id: &str) -> SavedConnection {
        SavedConnection::new(
            key, id.into(), "Inventory".into(), ConnectionDetails::Sqlite { path: SQLITE_PATH.into() }, "#22c55e".into(), "a".into(), "b".into(),
        )
        .unwrap()
    }

    #[test]
    fn sqlite_connections_keep_their_path_encrypted() {
        let key = [4u8; 32];
        let dir = temp_dir();
        let path = dir.join("db_connections.json");
        let mut conn = sqlite(&key, "lite");
        conn.read_only = true;
        conn.icon = "box".into();
        write_connections(&path, &[conn]).unwrap();

        let written = fs::read_to_string(&path).unwrap();
        for part in [SQLITE_PATH, "inventory-private", "/Users/me"] {
            assert!(!written.contains(part), "{part} is in the file: {written}");
        }
        let json: serde_json::Value = serde_json::from_str(&written).unwrap();
        let entry = &json["connections"][0];
        assert_eq!(entry["engine"], "sqlite");
        assert_eq!((entry["ssl"].as_bool(), entry["readOnly"].as_bool()), (Some(false), Some(true)));
        // The payload is just the path
        let payload: serde_json::Value = serde_json::from_str(&decrypt(&key, entry["encryptedData"].as_str().unwrap()).unwrap()).unwrap();
        assert_eq!(payload, serde_json::json!({ "path": SQLITE_PATH }));

        let loaded = load_connections_from_path(&path);
        assert_eq!(loaded[0].engine(), Some(Engine::Sqlite));
        let info = loaded[0].to_info(&key).unwrap();
        assert_eq!((info.engine, info.path.as_str()), (Engine::Sqlite, SQLITE_PATH));
        assert_eq!(
            (info.host.as_str(), info.port, info.database.as_str(), info.user.as_str(), info.password.as_str()),
            ("", 0, "", "", "")
        );
        assert!(info.ssh.is_none() && info.tls.is_none() && !info.ssl && info.read_only);
        assert_eq!((info.id.as_str(), info.name.as_str(), info.color.as_str(), info.icon.as_str()), ("lite", "Inventory", "#22c55e", "box"));
        assert_eq!((info.created_at.as_str(), info.updated_at.as_str()), ("a", "b"));
        let info = serde_json::to_value(&info).unwrap();
        assert_eq!((info["engine"].as_str(), info["path"].as_str()), (Some("sqlite"), Some(SQLITE_PATH)));

        // Written back by the next save with its engine
        write_connections(&path, &loaded).unwrap();
        let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["connections"][0]["engine"], "sqlite");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn postgres_connections_still_round_trip() {
        let key = [6u8; 32];
        let dir = temp_dir();
        let path = dir.join("db_connections.json");
        let plain = connection(&key, "plain");
        // The payload is written exactly as before SQLite existed
        assert_eq!(
            decrypt(&key, &plain.encrypted_data).unwrap(),
            r#"{"host":"db.example.com","port":5432,"database":"shop","user":"me","password":"secret","ssh":null,"tls":null}"#
        );
        let ssh = SshConfig {
            host: "bastion".into(),
            port: 2222,
            user: "ubuntu".into(),
            auth_method: "key".into(),
            password: String::new(),
            key_path: "/home/me/.ssh/id_ed25519".into(),
            passphrase: "pp".into(),
        };
        let tls = TlsSettings { mode: "verify-full".into(), root_cert: "/ca.pem".into(), cert: "/client.pem".into(), key: "/client.key".into() };
        let full = SavedConnection::new(
            &key,
            "full".into(),
            "Prod".into(),
            ConnectionDetails::Postgres {
                host: "db.example.com".into(),
                port: 5433,
                database: "shop".into(),
                user: "me".into(),
                password: "secret".into(),
                ssh: Some(ssh),
                tls: Some(tls),
                ssl: true,
            },
            "#ef4444".into(),
            "t1".into(),
            "t2".into(),
        )
        .unwrap();
        write_connections(&path, &[plain, full]).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(!written.contains("db.example.com") && !written.contains("secret") && !written.contains("bastion"), "{written}");

        let loaded = load_connections_from_path(&path);
        let info = loaded[1].to_info(&key).unwrap();
        assert_eq!((info.engine, info.path.as_str(), info.ssl), (Engine::Postgres, "", true));
        assert_eq!(
            (info.host.as_str(), info.port, info.database.as_str(), info.user.as_str(), info.password.as_str()),
            ("db.example.com", 5433, "shop", "me", "secret")
        );
        let ssh = info.ssh.expect("the ssh settings");
        assert_eq!((ssh.host.as_str(), ssh.port, ssh.key_path.as_str(), ssh.passphrase.as_str()), ("bastion", 2222, "/home/me/.ssh/id_ed25519", "pp"));
        let tls = info.tls.expect("the tls settings");
        assert_eq!((tls.mode.as_str(), tls.root_cert.as_str(), tls.cert.as_str(), tls.key.as_str()), ("verify-full", "/ca.pem", "/client.pem", "/client.key"));
        let plain = loaded[0].to_info(&key).unwrap();
        assert_eq!((plain.host.as_str(), plain.path.as_str(), plain.ssl), ("db.example.com", "", false));
        assert!(plain.ssh.is_none() && plain.tls.is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_mixing_engines_offers_the_known_ones_and_keeps_the_rest() {
        let key = [8u8; 32];
        let dir = temp_dir();
        let path = dir.join("db_connections.json");
        let (unknown_file, _) = file_with_an_unknown_engine(&key);
        let mut file: serde_json::Value = serde_json::from_str(&unknown_file).unwrap();
        let sqlite_payload = encrypt(&key, &format!(r#"{{"path":"{SQLITE_PATH}"}}"#)).unwrap();
        file["connections"].as_array_mut().unwrap().insert(
            1,
            serde_json::json!({
                "id": "lite", "name": "Inventory", "engine": "sqlite", "encryptedData": sqlite_payload, "ssl": false,
                "readOnly": true, "color": "#22c55e", "icon": "box", "createdAt": "t5", "updatedAt": "t6"
            }),
        );
        let original_unknown = file["connections"][2].clone();
        assert_eq!(original_unknown["engine"], "mysql");
        fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();

        let loaded = load_connections_from_path(&path);
        assert!(path.exists(), "the file isn't moved aside");
        assert_eq!(loaded.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["pg", "lite", "my"]);
        // What get_saved_connections offers
        let offered: Vec<_> = loaded.iter().filter_map(|c| c.to_info(&key).ok()).collect();
        assert_eq!(
            offered.iter().map(|c| (c.id.as_str(), c.engine)).collect::<Vec<_>>(),
            [("pg", Engine::Postgres), ("lite", Engine::Sqlite)]
        );
        assert_eq!((offered[0].host.as_str(), offered[0].path.as_str()), ("db.example.com", ""));
        assert_eq!((offered[1].host.as_str(), offered[1].path.as_str(), offered[1].read_only), ("", SQLITE_PATH, true));

        write_connections(&path, &loaded).unwrap();
        let written: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["connections"][2], original_unknown, "the unknown connection is written back as it was");
        assert_eq!(serde_json::to_string(&written["connections"][2]).unwrap(), serde_json::to_string(&original_unknown).unwrap());
        assert_eq!(written["connections"][1]["engine"], "sqlite");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sqlite_connections_under_the_hostname_key_move_to_the_new_key() {
        let (legacy, key) = ([1u8; 32], [2u8; 32]);
        let mut connections = vec![sqlite(&legacy, "old"), connection(&key, "new")];
        assert!(connections[0].to_info(&key).is_err());
        assert!(migrate_to_key(&mut connections, &key, &legacy));
        let info = connections[0].to_info(&key).unwrap();
        assert_eq!((info.engine, info.path.as_str()), (Engine::Sqlite, SQLITE_PATH));
        assert_eq!(connections[1].to_info(&key).unwrap().password, "secret");
        assert!(!migrate_to_key(&mut connections, &key, &legacy), "nothing left to move");
    }
}
