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

/// Derives a 32-byte AES key from the machine hostname using PBKDF2-SHA256.
/// Consistent across restarts on the same machine without any external storage.
pub fn get_or_create_key() -> Result<[u8; 32], String> {
    use pbkdf2::pbkdf2_hmac;
    use sha2::Sha256;

    let machine_name = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "dbunny-default-host".to_string());

    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(machine_name.as_bytes(), PBKDF2_SALT, PBKDF2_ITERATIONS, &mut key);
    Ok(key)
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

pub fn load_connections_from_path(path: &Path) -> Vec<SavedConnection> {
    if !path.exists() {
        return Vec::new();
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let file: ConnectionsFile = serde_json::from_str(&content).unwrap_or(ConnectionsFile {
        version: 2,
        connections: Vec::new(),
    });

    file.connections
}

pub fn save_connections_to_file(
    app_handle: &tauri::AppHandle,
    connections: &[SavedConnection],
) -> Result<(), String> {
    let path = get_storage_path(app_handle)?;

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
    fs::rename(&temp_path, &path)
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
}
