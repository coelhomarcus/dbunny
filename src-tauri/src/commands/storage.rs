use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

use super::ssh::SshConfig;

const PBKDF2_SALT: &[u8] = b"dbunny-connection-salt-v1";
const PBKDF2_ITERATIONS: u32 = 100_000;

/// Sensitive connection data — encrypted at rest as a JSON blob.
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

/// SSL settings beyond on/off, saved with a connection.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TlsSettings {
    /// "require", "verify-ca" or "verify-full".
    pub mode: String,
    #[serde(default)]
    pub root_cert: String,
    #[serde(default)]
    pub cert: String,
    #[serde(default)]
    pub key: String,
}

/// Stored on disk. Only metadata is plaintext; everything else is in `encrypted_data`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
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
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    pub ssh: Option<SshConfig>,
    pub tls: Option<TlsSettings>,
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

impl SavedConnection {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        key: &[u8; 32],
        id: String,
        name: String,
        host: String,
        port: u16,
        database: String,
        user: String,
        password: String,
        ssh: Option<SshConfig>,
        tls: Option<TlsSettings>,
        ssl: bool,
        color: String,
        created_at: String,
        updated_at: String,
    ) -> Result<Self, String> {
        let sensitive = SensitiveData {
            host,
            port,
            database,
            user,
            password,
            ssh,
            tls,
        };
        let json = serde_json::to_string(&sensitive)
            .map_err(|e| format!("Failed to serialize sensitive data: {e}"))?;
        let encrypted_data = encrypt(key, &json)?;

        Ok(Self {
            id,
            name,
            encrypted_data,
            ssl,
            read_only: false,
            color,
            icon: String::new(),
            created_at,
            updated_at,
        })
    }

    pub fn to_info(&self, key: &[u8; 32]) -> Result<SavedConnectionInfo, String> {
        let json = decrypt(key, &self.encrypted_data)?;
        let sensitive: SensitiveData = serde_json::from_str(&json)
            .map_err(|e| format!("Failed to deserialize sensitive data: {e}"))?;

        Ok(SavedConnectionInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            host: sensitive.host,
            port: sensitive.port,
            database: sensitive.database,
            user: sensitive.user,
            password: sensitive.password,
            ssh: sensitive.ssh,
            tls: sensitive.tls,
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
            &key, "1".into(), "Prod".into(), "db.example.com".into(), 5432, "shop".into(), "me".into(), "secret".into(),
            None, None, true, "#ef4444".into(), "a".into(), "b".into(),
        )
        .unwrap();
        conn.icon = "rocket".into();
        let stored = serde_json::to_string(&conn).unwrap();
        assert!(stored.contains(r#""icon":"rocket""#) && !stored.contains("secret"), "{stored}");
        let info = serde_json::from_str::<SavedConnection>(&stored).unwrap().to_info(&key).unwrap();
        assert_eq!((info.icon.as_str(), info.host.as_str()), ("rocket", "db.example.com"));
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dbunny-storage-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn connection(key: &[u8; 32], name: &str) -> SavedConnection {
        SavedConnection::new(
            key, name.into(), name.into(), "db.example.com".into(), 5432, "shop".into(), "me".into(), "secret".into(),
            None, None, false, "#3b82f6".into(), "a".into(), "b".into(),
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
}
