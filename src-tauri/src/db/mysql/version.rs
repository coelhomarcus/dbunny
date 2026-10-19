//! The server version, read from `VERSION()`.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MysqlVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl MysqlVersion {
    /// Reads `VERSION()`; refuses MariaDB and anything older than 8.0.
    pub fn parse(version: &str) -> Result<MysqlVersion, String> {
        let text = version.trim();
        if text.to_ascii_lowercase().contains("mariadb") {
            // Older MariaDB servers prefix the real version with 5.5.5-
            let real = text.strip_prefix("5.5.5-").unwrap_or(text);
            let shown = numbers(real).map(|(_, shown)| shown).unwrap_or_else(|| real.to_string());
            return Err(format!("This server is MariaDB {shown}, which DBunny doesn't support yet."));
        }
        let (parsed, shown) = numbers(text).ok_or_else(|| format!("Couldn't read the MySQL version from \"{text}\"."))?;
        if parsed.major < 8 {
            return Err(format!("MySQL {shown} isn't supported; DBunny needs MySQL 8.0 or newer."));
        }
        Ok(parsed)
    }

    /// Whether this version is `major.minor.patch` or newer.
    pub fn at_least(self, major: u16, minor: u16, patch: u16) -> bool {
        self >= MysqlVersion { major, minor, patch }
    }
}

/// The leading `major.minor.patch` of `text` and how it reads ("8.4.11"). A missing
/// minor or patch counts as 0.
fn numbers(text: &str) -> Option<(MysqlVersion, String)> {
    let end = text.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(text.len());
    let shown = text[..end].trim_end_matches('.');
    let mut parts = shown.split('.').map(|part| part.parse::<u16>());
    let major = parts.next()?.ok()?;
    let minor = parts.next().unwrap_or(Ok(0)).ok()?;
    let patch = parts.next().unwrap_or(Ok(0)).ok()?;
    Some((MysqlVersion { major, minor, patch }, shown.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_from_eight_on_are_accepted() {
        assert_eq!(MysqlVersion::parse("8.4.11").unwrap(), MysqlVersion { major: 8, minor: 4, patch: 11 });
        assert_eq!(MysqlVersion::parse("8.0.36-0ubuntu0.22.04.1").unwrap().patch, 36);
        assert!(MysqlVersion::parse("9.1.0").is_ok());
    }

    #[test]
    fn old_mysql_and_mariadb_are_refused_by_name() {
        assert_eq!(MysqlVersion::parse("5.7.44-log").err().unwrap(), "MySQL 5.7.44 isn't supported; DBunny needs MySQL 8.0 or newer.");
        assert_eq!(MysqlVersion::parse("11.4.13-MariaDB-ubu2404").err().unwrap(), "This server is MariaDB 11.4.13, which DBunny doesn't support yet.");
        // Older MariaDB servers prefix the version with 5.5.5-
        assert_eq!(MysqlVersion::parse("5.5.5-10.6.18-MariaDB").err().unwrap(), "This server is MariaDB 10.6.18, which DBunny doesn't support yet.");
    }
}
