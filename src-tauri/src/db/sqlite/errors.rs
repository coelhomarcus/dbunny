//! SQLite errors in words the app shows.

use std::path::Path;

use rusqlite::ffi::ErrorCode;

/// The same text PostgreSQL gives a cancelled query, so the frontend treats both alike.
pub const CANCELLED: &str = "canceling statement due to user request";

/// What the user reads for `e`, raised on the file at `path`. `editor_in_transaction`
/// says the query editor holds a transaction open, which is then what locks the file.
pub fn map_error(e: &rusqlite::Error, path: &Path, editor_in_transaction: bool) -> String {
    let rusqlite::Error::SqliteFailure(failure, _) = e else {
        return e.to_string();
    };
    match failure.code {
        ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked if editor_in_transaction => {
            "A transaction in the query editor is holding the database: commit or roll it back, then try again.".into()
        }
        ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => {
            "The database is locked by another program. Try again when it's done.".into()
        }
        ErrorCode::NotADatabase => format!("{} isn't a SQLite database.", path.display()),
        ErrorCode::OperationInterrupted => CANCELLED.into(),
        ErrorCode::CannotOpen | ErrorCode::PermissionDenied => match std::fs::metadata(path) {
            Err(io) if io.kind() == std::io::ErrorKind::NotFound => missing(path),
            _ => cannot_open(path, &system_reason(path).unwrap_or_else(|| e.to_string())),
        },
        _ => e.to_string(),
    }
}

/// "The file {path} doesn't exist."
pub fn missing(path: &Path) -> String {
    format!("The file {} doesn't exist.", path.display())
}

/// "The file {path} can't be opened: {reason}"
pub fn cannot_open(path: &Path, reason: &str) -> String {
    format!("The file {} can't be opened: {reason}", path.display())
}

/// Why the system refuses to open `path`, if it does.
fn system_reason(path: &Path) -> Option<String> {
    std::fs::File::open(path).err().map(|io| io.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::ffi;

    fn failure(code: i32) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(ffi::Error::new(code), None)
    }

    #[test]
    fn locks_name_who_holds_the_database() {
        let path = Path::new("/x/a.sqlite");
        assert_eq!(
            map_error(&failure(ffi::SQLITE_BUSY), path, true),
            "A transaction in the query editor is holding the database: commit or roll it back, then try again."
        );
        assert_eq!(
            map_error(&failure(ffi::SQLITE_BUSY), path, false),
            "The database is locked by another program. Try again when it's done."
        );
        assert_eq!(
            map_error(&failure(ffi::SQLITE_LOCKED), path, false),
            "The database is locked by another program. Try again when it's done."
        );
    }

    #[test]
    fn cancelled_and_wrong_files_read_like_the_app() {
        let path = Path::new("/x/a.sqlite");
        assert_eq!(map_error(&failure(ffi::SQLITE_INTERRUPT), path, false), "canceling statement due to user request");
        assert_eq!(map_error(&failure(ffi::SQLITE_NOTADB), path, false), "/x/a.sqlite isn't a SQLite database.");
        assert_eq!(map_error(&failure(ffi::SQLITE_CANTOPEN), path, false), "The file /x/a.sqlite doesn't exist.");
    }
}
