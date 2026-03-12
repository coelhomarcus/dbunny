use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

const PBKDF2_ITERATIONS: u32 = 100_000;
const SALT: &[u8] = b"dbunny-connection-salt-v1";

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub encrypted_password: String,
    pub ssl: bool,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

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
    pub ssl: bool,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize)]
pub struct ConnectionsFile {
    pub version: u32,
    pub connections: Vec<SavedConnection>,
}

/// Derives AES-256 key from machine hostname. Call once at startup.
pub fn derive_key() -> [u8; 32] {
    let machine_name = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "dbunny-default-host".to_string());

    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(machine_name.as_bytes(), SALT, PBKDF2_ITERATIONS, &mut key);
    key
}

pub fn encrypt_password(key: &[u8; 32], plaintext: &str) -> Result<String, String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| format!("Failed to create cipher: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(BASE64.encode(&combined))
}

pub fn decrypt_password(key: &[u8; 32], encrypted: &str) -> Result<String, String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| format!("Failed to create cipher: {}", e))?;

    let combined = BASE64
        .decode(encrypted)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    if combined.len() < 13 {
        return Err("Invalid encrypted data".into());
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext).map_err(|e| format!("Invalid UTF-8: {}", e))
}

pub fn get_storage_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    Ok(app_data_dir.join("connections.json"))
}

/// Load connections from a specific path. Used during setup before AppHandle is fully available.
pub fn load_connections_from_path(path: &Path) -> Vec<SavedConnection> {
    if !path.exists() {
        return Vec::new();
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let file: ConnectionsFile = serde_json::from_str(&content).unwrap_or(ConnectionsFile {
        version: 1,
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
            .map_err(|e| format!("Failed to create app data dir: {}", e))?;
    }

    let file = ConnectionsFile {
        version: 1,
        connections: connections.to_vec(),
    };

    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("Failed to serialize connections: {}", e))?;

    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, &json)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    fs::rename(&temp_path, &path)
        .map_err(|e| format!("Failed to rename temp file: {}", e))?;

    Ok(())
}

impl SavedConnection {
    pub fn to_info(&self, key: &[u8; 32]) -> Result<SavedConnectionInfo, String> {
        let password = if self.encrypted_password.is_empty() {
            String::new()
        } else {
            decrypt_password(key, &self.encrypted_password)?
        };

        Ok(SavedConnectionInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port,
            database: self.database.clone(),
            user: self.user.clone(),
            password,
            ssl: self.ssl,
            color: self.color.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        })
    }
}
