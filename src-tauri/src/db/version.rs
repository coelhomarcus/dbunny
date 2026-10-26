//! The version of a database server: MySQL and MariaDB read it from `VERSION()`,
//! PostgreSQL from `server_version_num`.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServerVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl ServerVersion {
    /// Whether this version is `major.minor.patch` or newer.
    pub fn at_least(self, major: u16, minor: u16, patch: u16) -> bool {
        self >= ServerVersion { major, minor, patch }
    }

    // Only the server tests read it until the PostgreSQL driver knows its version
    #[cfg_attr(not(test), allow(dead_code))]
    /// PostgreSQL's `server_version_num`: 170002 is 17.2, and before 10 three parts, 90624 is 9.6.24.
    pub fn from_postgres_num(num: i32) -> ServerVersion {
        let num = num.max(0);
        let part = |n: i32| u16::try_from(n).unwrap_or(u16::MAX);
        if num >= 100_000 {
            ServerVersion { major: part(num / 10_000), minor: part(num % 10_000), patch: 0 }
        } else {
            ServerVersion { major: part(num / 10_000), minor: part(num / 100 % 100), patch: part(num % 100) }
        }
    }
}

/// The line a server test prints when it skips because the server is too old for what it
/// checks; `matrix.sh` counts these lines.
#[cfg(test)]
pub fn skip_line(test: &str, engine: crate::db::Engine, needed: ServerVersion) -> String {
    let version = if needed.patch == 0 { format!("{}.{}", needed.major, needed.minor) } else { needed.to_string() };
    format!("dbunny-skip: {test} needs {} {version}", engine.label())
}

impl fmt::Display for ServerVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The leading `major.minor.patch` of `text` and how it reads ("8.4.11"). A missing
/// minor or patch counts as 0.
pub(crate) fn numbers(text: &str) -> Option<(ServerVersion, String)> {
    let end = text.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(text.len());
    let shown = text[..end].trim_end_matches('.');
    let mut parts = shown.split('.').map(|part| part.parse::<u16>());
    let major = parts.next()?.ok()?;
    let minor = parts.next().unwrap_or(Ok(0)).ok()?;
    let patch = parts.next().unwrap_or(Ok(0)).ok()?;
    Some((ServerVersion { major, minor, patch }, shown.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Engine;

    #[test]
    fn reads_major_minor_patch_and_shows_it() {
        let (version, shown) = numbers("8.0.36-0ubuntu0.22.04.1").unwrap();
        assert_eq!(version, ServerVersion { major: 8, minor: 0, patch: 36 });
        assert_eq!(shown, "8.0.36");
        assert_eq!(version.to_string(), "8.0.36");
        assert_eq!(numbers("10.6").unwrap().0, ServerVersion { major: 10, minor: 6, patch: 0 });
        assert!(numbers("MariaDB").is_none());
    }

    #[test]
    fn postgres_version_numbers_are_read() {
        assert_eq!(ServerVersion::from_postgres_num(170002), ServerVersion { major: 17, minor: 2, patch: 0 });
        assert_eq!(ServerVersion::from_postgres_num(130023), ServerVersion { major: 13, minor: 23, patch: 0 });
        assert_eq!(ServerVersion::from_postgres_num(90624), ServerVersion { major: 9, minor: 6, patch: 24 });
    }

    #[test]
    fn skip_lines_name_the_test_and_the_version() {
        let v = |major, minor, patch| ServerVersion { major, minor, patch };
        assert_eq!(skip_line("grant_maintain", Engine::Postgres, v(17, 0, 0)), "dbunny-skip: grant_maintain needs PostgreSQL 17.0");
        assert_eq!(skip_line("explain_analyze", Engine::Mysql, v(8, 0, 18)), "dbunny-skip: explain_analyze needs MySQL 8.0.18");
    }

    #[test]
    fn versions_compare_by_parts() {
        let version = ServerVersion { major: 8, minor: 0, patch: 18 };
        assert!(version.at_least(8, 0, 18));
        assert!(!version.at_least(8, 0, 19));
        assert!(version.at_least(7, 9, 99));
    }
}
