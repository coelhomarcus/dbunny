//! Reading the version of a MariaDB server.

use crate::db::mysql_common::version::{numbers, ServerVersion};

/// Reads `VERSION()` ("11.4.13-MariaDB-ubu2404"); refuses MySQL and anything older than 10.6.
pub fn parse_mariadb_version(version: &str) -> Result<ServerVersion, String> {
    let text = version.trim();
    if !text.to_ascii_lowercase().contains("mariadb") {
        let (_, shown) = numbers(text).ok_or_else(|| format!("Couldn't read the MariaDB version from \"{text}\"."))?;
        return Err(format!("This server is MySQL {shown}. Edit the connection and choose MySQL."));
    }
    // Older MariaDB servers prefix the real version with 5.5.5-
    let real = text.strip_prefix("5.5.5-").unwrap_or(text);
    let (parsed, shown) = numbers(real).ok_or_else(|| format!("Couldn't read the MariaDB version from \"{text}\"."))?;
    if (parsed.major, parsed.minor) < (10, 6) {
        return Err(format!("MariaDB {shown} isn't supported; DBunny needs MariaDB 10.6 or newer."));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mariadb_versions_are_read() {
        assert_eq!(parse_mariadb_version("11.4.13-MariaDB-ubu2404").unwrap(), ServerVersion { major: 11, minor: 4, patch: 13 });
        assert_eq!(parse_mariadb_version("10.6.18-MariaDB").unwrap(), ServerVersion { major: 10, minor: 6, patch: 18 });
        // Older servers prefix the version with 5.5.5-
        assert_eq!(parse_mariadb_version("5.5.5-10.11.8-MariaDB-log").unwrap(), ServerVersion { major: 10, minor: 11, patch: 8 });
    }

    #[test]
    fn mariadb_refuses_mysql_and_old_versions() {
        assert_eq!(parse_mariadb_version("8.4.11").err().unwrap(), "This server is MySQL 8.4.11. Edit the connection and choose MySQL.");
        assert_eq!(parse_mariadb_version("5.7.44-log").err().unwrap(), "This server is MySQL 5.7.44. Edit the connection and choose MySQL.");
        assert_eq!(parse_mariadb_version("10.5.29-MariaDB").err().unwrap(), "MariaDB 10.5.29 isn't supported; DBunny needs MariaDB 10.6 or newer.");
        assert_eq!(parse_mariadb_version("5.5.5-10.3.39-MariaDB").err().unwrap(), "MariaDB 10.3.39 isn't supported; DBunny needs MariaDB 10.6 or newer.");
        assert_eq!(parse_mariadb_version("gibberish").err().unwrap(), "Couldn't read the MariaDB version from \"gibberish\".");
    }
}
