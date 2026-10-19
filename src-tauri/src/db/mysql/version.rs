//! Reading the version of a MySQL server.

use crate::db::mysql_common::version::{numbers, ServerVersion};

/// Reads `VERSION()`; refuses MariaDB and anything older than 8.0.
pub fn parse_mysql_version(version: &str) -> Result<ServerVersion, String> {
    let text = version.trim();
    if text.to_ascii_lowercase().contains("mariadb") {
        // Older MariaDB servers prefix the real version with 5.5.5-
        let real = text.strip_prefix("5.5.5-").unwrap_or(text);
        let shown = numbers(real).map(|(_, shown)| shown).unwrap_or_else(|| real.to_string());
        return Err(format!("This server is MariaDB {shown}. Edit the connection and choose MariaDB."));
    }
    let (parsed, shown) = numbers(text).ok_or_else(|| format!("Couldn't read the MySQL version from \"{text}\"."))?;
    if parsed.major < 8 {
        return Err(format!("MySQL {shown} isn't supported; DBunny needs MySQL 8.0 or newer."));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_from_eight_on_are_accepted() {
        assert_eq!(parse_mysql_version("8.4.11").unwrap(), ServerVersion { major: 8, minor: 4, patch: 11 });
        assert_eq!(parse_mysql_version("8.0.36-0ubuntu0.22.04.1").unwrap().patch, 36);
        assert!(parse_mysql_version("9.1.0").is_ok());
    }

    #[test]
    fn old_mysql_and_mariadb_are_refused_by_name() {
        assert_eq!(parse_mysql_version("5.7.44-log").err().unwrap(), "MySQL 5.7.44 isn't supported; DBunny needs MySQL 8.0 or newer.");
        assert_eq!(parse_mysql_version("11.4.13-MariaDB-ubu2404").err().unwrap(), "This server is MariaDB 11.4.13. Edit the connection and choose MariaDB.");
        // Older MariaDB servers prefix the version with 5.5.5-
        assert_eq!(parse_mysql_version("5.5.5-10.6.18-MariaDB").err().unwrap(), "This server is MariaDB 10.6.18. Edit the connection and choose MariaDB.");
    }
}
