//! The server version, read from `VERSION()`.

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
    fn versions_compare_by_parts() {
        let version = ServerVersion { major: 8, minor: 0, patch: 18 };
        assert!(version.at_least(8, 0, 18));
        assert!(!version.at_least(8, 0, 19));
        assert!(version.at_least(7, 9, 99));
    }
}
