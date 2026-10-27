//! The version of a database server: MySQL and MariaDB read it from `VERSION()`,
//! PostgreSQL from `server_version_num`.

use std::fmt;

use crate::db::Engine;

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

/// When a version line stopped (or stops) being maintained by its vendor: `line` is the server's
/// own line ("13", "8.0"), `date` the last day of support (YYYY-MM-DD).
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndOfSupport {
    pub line: String,
    pub date: String,
}

/// Every version line since the oldest one DBunny opens, and its last day of support
/// (endoflife.date, 2026-10-26), short-lived ones (MySQL Innovation, MariaDB rolling) included.
/// PostgreSQL lines are the major version (the minor is ignored); MySQL and MariaDB lines are
/// major.minor. In version order within each engine.
const SUPPORT: &[(Engine, u16, u16, &str)] = &[
    (Engine::Postgres, 13, 0, "2025-11-13"),
    (Engine::Postgres, 14, 0, "2026-11-12"),
    (Engine::Postgres, 15, 0, "2027-11-11"),
    (Engine::Postgres, 16, 0, "2028-11-09"),
    (Engine::Postgres, 17, 0, "2029-11-08"),
    (Engine::Postgres, 18, 0, "2030-11-14"),
    (Engine::Mysql, 8, 0, "2026-04-30"),
    (Engine::Mysql, 8, 1, "2023-10-25"),
    (Engine::Mysql, 8, 2, "2024-01-16"),
    (Engine::Mysql, 8, 3, "2024-04-30"),
    (Engine::Mysql, 8, 4, "2032-04-30"),
    (Engine::Mysql, 9, 0, "2024-10-15"),
    (Engine::Mysql, 9, 1, "2025-01-21"),
    (Engine::Mysql, 9, 2, "2025-04-15"),
    (Engine::Mysql, 9, 3, "2025-07-22"),
    (Engine::Mysql, 9, 4, "2025-10-21"),
    (Engine::Mysql, 9, 5, "2026-01-20"),
    (Engine::Mysql, 9, 6, "2026-04-21"),
    (Engine::Mysql, 9, 7, "2034-04-30"),
    (Engine::Mysql, 26, 7, "2026-10-20"),
    (Engine::Mariadb, 10, 6, "2026-07-06"),
    (Engine::Mariadb, 10, 7, "2023-02-09"),
    (Engine::Mariadb, 10, 8, "2023-05-20"),
    (Engine::Mariadb, 10, 9, "2023-08-22"),
    (Engine::Mariadb, 10, 10, "2023-11-17"),
    (Engine::Mariadb, 10, 11, "2028-02-16"),
    (Engine::Mariadb, 11, 0, "2024-06-06"),
    (Engine::Mariadb, 11, 1, "2024-08-21"),
    (Engine::Mariadb, 11, 2, "2024-11-21"),
    (Engine::Mariadb, 11, 3, "2024-05-29"),
    (Engine::Mariadb, 11, 4, "2029-05-29"),
    (Engine::Mariadb, 11, 5, "2024-11-21"),
    (Engine::Mariadb, 11, 6, "2025-02-13"),
    (Engine::Mariadb, 11, 7, "2025-05-12"),
    (Engine::Mariadb, 11, 8, "2028-06-04"),
    (Engine::Mariadb, 12, 0, "2025-11-18"),
    (Engine::Mariadb, 12, 1, "2026-02-13"),
    (Engine::Mariadb, 12, 2, "2026-05-28"),
    (Engine::Mariadb, 12, 3, "2029-06-12"),
    (Engine::Mariadb, 13, 0, "2026-12-31"),
];

/// The last day `version`'s line is maintained. A version newer than every line of its engine
/// (a MySQL Innovation or MariaDB rolling release, a PostgreSQL out after this table) has none;
/// one the table doesn't list between two lines takes the date of the line before it, the
/// longest that can be said for sure. SQLite has none.
pub fn end_of_support(engine: Engine, version: ServerVersion) -> Option<EndOfSupport> {
    let (key, line) = match engine {
        Engine::Sqlite => return None,
        Engine::Postgres => ((version.major, 0), version.major.to_string()),
        Engine::Mysql | Engine::Mariadb => ((version.major, version.minor), format!("{}.{}", version.major, version.minor)),
    };
    let lines: Vec<_> = SUPPORT.iter().filter(|(e, ..)| *e == engine).collect();
    let last = lines.last()?;
    if key > (last.1, last.2) {
        return None;
    }
    // The line itself, or the newest one before it (or the oldest, below them all)
    let date = lines.iter().rev().find(|(_, major, minor, _)| (*major, *minor) <= key).or(lines.first())?.3;
    Some(EndOfSupport { line, date: date.to_string() })
}

/// `end_of_support`, only once `today` is past that last day.
pub fn past_end_of_support(engine: Engine, version: ServerVersion, today: chrono::NaiveDate) -> Option<EndOfSupport> {
    let support = end_of_support(engine, version)?;
    let last_day = chrono::NaiveDate::parse_from_str(&support.date, "%Y-%m-%d").ok()?;
    (today > last_day).then_some(support)
}

/// Whether the server is PostgreSQL itself, whose version lines the table follows. Compatible
/// servers report the PostgreSQL version they emulate (YugabyteDB 11.2, Greenplum 12, CockroachDB
/// 13.0, Redshift 8.0), not their own release, so neither the floor nor the end of support of
/// PostgreSQL applies to them. `server_version` is `SHOW server_version`, `version_text` is `version()`.
pub fn postgres_is_stock(server_version: &str, version_text: &str) -> bool {
    let number = server_version.split_whitespace().next().unwrap_or("");
    let plain_number = !number.is_empty() && number.chars().all(|c| c.is_ascii_digit() || c == '.');
    plain_number && version_text.starts_with("PostgreSQL ") && !["Greenplum", "Redshift"].iter().any(|name| version_text.contains(name))
}

/// Whether the server is MySQL itself. TiDB and Vitess (PlanetScale) write their name into
/// `VERSION()`; Aurora MySQL reports a plain 8.0.x and is told by `@@aurora_version` answering.
pub fn mysql_is_stock(version_text: &str, aurora: bool) -> bool {
    !aurora && !["TiDB", "Vitess"].iter().any(|name| version_text.contains(name))
}

/// The SQL a server's version speaks, by name, for the editor's suggestions and the assistant's
/// notes. An unknown version (or a compatible server) gets the oldest version DBunny opens.
pub fn sql_features(engine: Engine, version: Option<ServerVersion>) -> Vec<&'static str> {
    let oldest = match engine {
        Engine::Postgres => ServerVersion { major: 13, minor: 0, patch: 0 },
        Engine::Mysql => ServerVersion { major: 8, minor: 0, patch: 0 },
        Engine::Mariadb => ServerVersion { major: 10, minor: 6, patch: 0 },
        Engine::Sqlite => ServerVersion { major: 3, minor: 53, patch: 0 },
    };
    let v = version.unwrap_or(oldest);
    let mut features = vec!["windowFunctions", "recursiveCte"];
    match engine {
        Engine::Postgres => {
            features.extend(["returning", "returningUpdate", "onConflict", "nullsOrdering", "ilike", "lateral", "filterClause"]);
            if v.at_least(15, 0, 0) {
                features.extend(["merge", "nullsNotDistinct"]);
            }
            if v.at_least(16, 0, 0) {
                features.push("sqlJsonConstructors");
            }
            if v.at_least(17, 0, 0) {
                features.push("jsonTable");
            }
        }
        Engine::Mysql => {
            features.extend(["onDuplicateKeyUpdate", "jsonTable"]);
            if v.at_least(8, 0, 14) {
                features.push("lateral");
            }
        }
        Engine::Mariadb => features.extend(["returning", "onDuplicateKeyUpdate", "jsonTable"]),
        Engine::Sqlite => features.extend(["returning", "returningUpdate", "onConflict", "nullsOrdering", "filterClause"]),
    }
    features
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

    fn day(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn v(major: u16, minor: u16, patch: u16) -> ServerVersion {
        ServerVersion { major, minor, patch }
    }

    #[test]
    fn every_line_of_the_table_has_its_date() {
        let date = |engine, version| end_of_support(engine, version).map(|s| s.date);
        let pg = |major, expected: &str| assert_eq!(date(Engine::Postgres, v(major, 4, 0)).as_deref(), Some(expected), "PostgreSQL {major}");
        pg(13, "2025-11-13");
        pg(14, "2026-11-12");
        pg(15, "2027-11-11");
        pg(16, "2028-11-09");
        pg(17, "2029-11-08");
        pg(18, "2030-11-14");
        let line = |engine, major, minor, expected: &str| assert_eq!(date(engine, v(major, minor, 3)).as_deref(), Some(expected), "{engine:?} {major}.{minor}");
        line(Engine::Mysql, 8, 0, "2026-04-30");
        line(Engine::Mysql, 8, 4, "2032-04-30");
        line(Engine::Mysql, 9, 7, "2034-04-30");
        line(Engine::Mariadb, 10, 6, "2026-07-06");
        line(Engine::Mariadb, 10, 11, "2028-02-16");
        line(Engine::Mariadb, 11, 4, "2029-05-29");
        line(Engine::Mariadb, 11, 8, "2028-06-04");
        line(Engine::Mariadb, 12, 3, "2029-06-12");
    }

    #[test]
    fn the_last_day_is_still_supported() {
        assert_eq!(past_end_of_support(Engine::Postgres, v(14, 1, 0), day(2026, 11, 12)), None);
        assert_eq!(
            past_end_of_support(Engine::Postgres, v(14, 1, 0), day(2026, 11, 13)),
            Some(EndOfSupport { line: "14".into(), date: "2026-11-12".into() })
        );
    }

    #[test]
    fn newer_than_the_table_is_supported() {
        assert_eq!(end_of_support(Engine::Mysql, v(26, 10, 0)), None);
        assert_eq!(end_of_support(Engine::Mariadb, v(13, 1, 0)), None);
        assert_eq!(end_of_support(Engine::Postgres, v(19, 0, 0)), None);
        assert_eq!(end_of_support(Engine::Sqlite, v(3, 53, 0)), None);
        // A rolling release with a date still ahead
        assert_eq!(past_end_of_support(Engine::Mariadb, v(13, 0, 2), day(2026, 10, 26)), None);
    }

    #[test]
    fn short_lived_lines_have_their_own_date() {
        assert_eq!(end_of_support(Engine::Mariadb, v(11, 1, 0)), Some(EndOfSupport { line: "11.1".into(), date: "2024-08-21".into() }));
        assert_eq!(past_end_of_support(Engine::Mysql, v(9, 3, 0), day(2026, 10, 26)), Some(EndOfSupport { line: "9.3".into(), date: "2025-07-22".into() }));
        assert_eq!(past_end_of_support(Engine::Mysql, v(26, 7, 1), day(2026, 10, 26)), Some(EndOfSupport { line: "26.7".into(), date: "2026-10-20".into() }));
    }

    #[test]
    fn a_line_missing_from_the_table_takes_the_one_before() {
        // 9.8 never shipped: it sits between 9.7 and 26.7, so it gets 9.7's date
        assert_eq!(end_of_support(Engine::Mysql, v(9, 8, 0)), Some(EndOfSupport { line: "9.8".into(), date: "2034-04-30".into() }));
    }

    #[test]
    fn end_of_support_serializes_in_camel_case() {
        let json = serde_json::to_value(EndOfSupport { line: "13".into(), date: "2025-11-13".into() }).unwrap();
        assert_eq!(json, serde_json::json!({ "line": "13", "date": "2025-11-13" }));
    }

    #[test]
    fn stock_postgresql_is_told_apart_from_compatible_servers() {
        assert!(postgres_is_stock("13.23 (Debian 13.23-1.pgdg13+1)", "PostgreSQL 13.23 (Debian 13.23-1.pgdg13+1) on aarch64-unknown-linux-gnu, compiled by gcc"));
        assert!(postgres_is_stock("17.2", "PostgreSQL 17.2 on x86_64-pc-linux-gnu, compiled by gcc (GCC) 12.2.0, 64-bit"));
        // They report the PostgreSQL version they emulate, not their own release
        assert!(!postgres_is_stock("11.2-YB-2.20.1.0-b0", "PostgreSQL 11.2-YB-2.20.1.0-b0 on x86_64-pc-linux-gnu, compiled by clang"));
        assert!(!postgres_is_stock("12.12", "PostgreSQL 12.12 (Greenplum Database 7.1.0 build commit:e7c2b1f) on x86_64-pc-linux-gnu"));
        assert!(!postgres_is_stock("13.0.0", "CockroachDB CCL v23.1.11 (aarch64-unknown-linux-gnu, built 2023/09/27 01:53:43, go1.19.10)"));
        assert!(!postgres_is_stock("8.0.2", "PostgreSQL 8.0.2 on i686-pc-linux-gnu, compiled by GCC gcc (GCC) 3.4.2 20041017 (Red Hat 3.4.2-6.fc3), Redshift 1.0.77467"));
    }

    #[test]
    fn stock_mysql_is_told_apart_from_compatible_servers() {
        assert!(mysql_is_stock("8.0.46", false));
        assert!(mysql_is_stock("8.0.36-0ubuntu0.22.04.1", false));
        assert!(!mysql_is_stock("8.0.11-TiDB-v7.5.0", false));
        assert!(!mysql_is_stock("8.0.34-Vitess", false));
        // Aurora MySQL reports plain 8.0.x; it is told by @@aurora_version answering
        assert!(!mysql_is_stock("8.0.28", true));
    }

    #[test]
    fn sql_features_follow_engine_and_version() {
        let pg = |major| sql_features(Engine::Postgres, Some(v(major, 0, 0)));
        assert!(!pg(14).contains(&"merge") && pg(15).contains(&"merge") && pg(15).contains(&"nullsNotDistinct"));
        assert!(!pg(15).contains(&"sqlJsonConstructors") && pg(16).contains(&"sqlJsonConstructors"));
        assert!(!pg(16).contains(&"jsonTable") && pg(17).contains(&"jsonTable"));
        assert!(pg(13).contains(&"onConflict") && pg(13).contains(&"ilike") && !pg(13).contains(&"onDuplicateKeyUpdate"));
        let my = |patch| sql_features(Engine::Mysql, Some(v(8, 0, patch)));
        assert!(!my(13).contains(&"lateral") && my(14).contains(&"lateral"));
        assert!(!my(14).contains(&"returning") && my(14).contains(&"onDuplicateKeyUpdate") && my(14).contains(&"jsonTable"));
        let maria = sql_features(Engine::Mariadb, Some(v(10, 6, 0)));
        assert!(maria.contains(&"returning") && !maria.contains(&"returningUpdate") && !maria.contains(&"lateral"));
        let lite = sql_features(Engine::Sqlite, None);
        assert!(lite.contains(&"returningUpdate") && lite.contains(&"onConflict") && !lite.contains(&"ilike"));
        // Unknown version: the oldest accepted one
        assert_eq!(sql_features(Engine::Postgres, None), pg(13));
        assert_eq!(sql_features(Engine::Mysql, None), sql_features(Engine::Mysql, Some(v(8, 0, 0))));
    }
}
