//! The SSL settings saved with a connection and the reading of its certificate files.

use serde::{Deserialize, Serialize};

use crate::db::net::ssh::expand_home;

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

pub(crate) fn read_file(label: &str, path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(expand_home(path)).map_err(|e| format!("Couldn't read the {label} {path}: {e}"))
}
