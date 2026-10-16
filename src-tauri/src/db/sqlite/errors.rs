//! SQLite errors in words the app shows.

use std::ffi::c_int;
use std::path::Path;

use rusqlite::ffi::{self, ErrorCode};

/// The same text PostgreSQL gives a cancelled query, so the frontend treats both alike.
pub const CANCELLED: &str = "canceling statement due to user request";

/// What the user reads for `e`, raised while opening the session's file at `path`: only
/// then is a file that isn't a database, or can't be opened, the session's own file.
pub fn map_open_error(e: &rusqlite::Error, path: &Path) -> String {
    let rusqlite::Error::SqliteFailure(failure, _) = e else {
        return e.to_string();
    };
    match failure.code {
        ErrorCode::NotADatabase => format!("{} isn't a SQLite database.", path.display()),
        ErrorCode::CannotOpen => match std::fs::metadata(path) {
            Err(io) if io.kind() == std::io::ErrorKind::NotFound => missing(path),
            _ => cannot_open(path, &system_reason(path).unwrap_or_else(|| e.to_string())),
        },
        _ => map_statement_error(e, false),
    }
}

/// What the user reads for `e`, raised by a statement of an open session.
/// `editor_in_transaction` says the query editor holds a transaction open, which is then
/// what keeps the file busy. Errors about other files (an ATTACH, a temporary file) and
/// locks inside one connection keep SQLite's own text.
pub fn map_statement_error(e: &rusqlite::Error, editor_in_transaction: bool) -> String {
    let rusqlite::Error::SqliteFailure(failure, _) = e else {
        return e.to_string();
    };
    match failure.code {
        ErrorCode::DatabaseBusy if editor_in_transaction => {
            "A transaction in the query editor is holding the database: commit or roll it back, then try again.".into()
        }
        ErrorCode::DatabaseBusy => "The database is locked by another program. Try again when it's done.".into(),
        ErrorCode::OperationInterrupted => CANCELLED.into(),
        _ => e.to_string(),
    }
}

/// The name of a SQLite result code, such as `SQLITE_CONSTRAINT_NOTNULL`. An extended
/// code without a name here gets its primary code's name.
pub fn code_name(code: c_int) -> &'static str {
    macro_rules! names {
        ($code:expr; $($name:ident,)*) => {
            match $code {
                $(ffi::$name => Some(stringify!($name)),)*
                _ => None,
            }
        };
    }
    let named = |code: c_int| {
        names!(code;
        SQLITE_ERROR,
        SQLITE_INTERNAL,
        SQLITE_PERM,
        SQLITE_ABORT,
        SQLITE_BUSY,
        SQLITE_LOCKED,
        SQLITE_NOMEM,
        SQLITE_READONLY,
        SQLITE_INTERRUPT,
        SQLITE_IOERR,
        SQLITE_CORRUPT,
        SQLITE_NOTFOUND,
        SQLITE_FULL,
        SQLITE_CANTOPEN,
        SQLITE_PROTOCOL,
        SQLITE_EMPTY,
        SQLITE_SCHEMA,
        SQLITE_TOOBIG,
        SQLITE_CONSTRAINT,
        SQLITE_MISMATCH,
        SQLITE_MISUSE,
        SQLITE_NOLFS,
        SQLITE_AUTH,
        SQLITE_FORMAT,
        SQLITE_RANGE,
        SQLITE_NOTADB,
        SQLITE_NOTICE,
        SQLITE_WARNING,
        SQLITE_ERROR_MISSING_COLLSEQ,
        SQLITE_ERROR_RETRY,
        SQLITE_ERROR_SNAPSHOT,
        SQLITE_IOERR_READ,
        SQLITE_IOERR_SHORT_READ,
        SQLITE_IOERR_WRITE,
        SQLITE_IOERR_FSYNC,
        SQLITE_IOERR_DIR_FSYNC,
        SQLITE_IOERR_TRUNCATE,
        SQLITE_IOERR_FSTAT,
        SQLITE_IOERR_UNLOCK,
        SQLITE_IOERR_RDLOCK,
        SQLITE_IOERR_DELETE,
        SQLITE_IOERR_BLOCKED,
        SQLITE_IOERR_NOMEM,
        SQLITE_IOERR_ACCESS,
        SQLITE_IOERR_CHECKRESERVEDLOCK,
        SQLITE_IOERR_LOCK,
        SQLITE_IOERR_CLOSE,
        SQLITE_IOERR_DIR_CLOSE,
        SQLITE_IOERR_SHMOPEN,
        SQLITE_IOERR_SHMSIZE,
        SQLITE_IOERR_SHMLOCK,
        SQLITE_IOERR_SHMMAP,
        SQLITE_IOERR_SEEK,
        SQLITE_IOERR_DELETE_NOENT,
        SQLITE_IOERR_MMAP,
        SQLITE_IOERR_GETTEMPPATH,
        SQLITE_IOERR_CONVPATH,
        SQLITE_IOERR_VNODE,
        SQLITE_IOERR_AUTH,
        SQLITE_IOERR_BEGIN_ATOMIC,
        SQLITE_IOERR_COMMIT_ATOMIC,
        SQLITE_IOERR_ROLLBACK_ATOMIC,
        SQLITE_IOERR_DATA,
        SQLITE_IOERR_CORRUPTFS,
        SQLITE_IOERR_IN_PAGE,
        SQLITE_LOCKED_SHAREDCACHE,
        SQLITE_LOCKED_VTAB,
        SQLITE_BUSY_RECOVERY,
        SQLITE_BUSY_SNAPSHOT,
        SQLITE_BUSY_TIMEOUT,
        SQLITE_CANTOPEN_NOTEMPDIR,
        SQLITE_CANTOPEN_ISDIR,
        SQLITE_CANTOPEN_FULLPATH,
        SQLITE_CANTOPEN_CONVPATH,
        SQLITE_CANTOPEN_DIRTYWAL,
        SQLITE_CANTOPEN_SYMLINK,
        SQLITE_CORRUPT_VTAB,
        SQLITE_CORRUPT_SEQUENCE,
        SQLITE_CORRUPT_INDEX,
        SQLITE_READONLY_RECOVERY,
        SQLITE_READONLY_CANTLOCK,
        SQLITE_READONLY_ROLLBACK,
        SQLITE_READONLY_DBMOVED,
        SQLITE_READONLY_CANTINIT,
        SQLITE_READONLY_DIRECTORY,
        SQLITE_ABORT_ROLLBACK,
        SQLITE_CONSTRAINT_CHECK,
        SQLITE_CONSTRAINT_COMMITHOOK,
        SQLITE_CONSTRAINT_FOREIGNKEY,
        SQLITE_CONSTRAINT_FUNCTION,
        SQLITE_CONSTRAINT_NOTNULL,
        SQLITE_CONSTRAINT_PRIMARYKEY,
        SQLITE_CONSTRAINT_TRIGGER,
        SQLITE_CONSTRAINT_UNIQUE,
        SQLITE_CONSTRAINT_VTAB,
        SQLITE_CONSTRAINT_ROWID,
        SQLITE_CONSTRAINT_PINNED,
        SQLITE_CONSTRAINT_DATATYPE,
        SQLITE_NOTICE_RECOVER_WAL,
        SQLITE_NOTICE_RECOVER_ROLLBACK,
        SQLITE_WARNING_AUTOINDEX,
        SQLITE_AUTH_USER,
        )
    };
    named(code).or_else(|| named(code & 0xff)).unwrap_or("SQLITE_ERROR")
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
    use super::super::tests_support::TempDb;
    use super::*;

    fn failure(code: i32) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(ffi::Error::new(code), None)
    }

    fn failure_with(code: i32, message: &str) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(ffi::Error::new(code), Some(message.into()))
    }

    #[test]
    fn locks_name_who_holds_the_database() {
        assert_eq!(
            map_statement_error(&failure(ffi::SQLITE_BUSY), true),
            "A transaction in the query editor is holding the database: commit or roll it back, then try again."
        );
        assert_eq!(
            map_statement_error(&failure(ffi::SQLITE_BUSY), false),
            "The database is locked by another program. Try again when it's done."
        );
        let path = Path::new("/x/a.sqlite");
        assert_eq!(
            map_open_error(&failure(ffi::SQLITE_BUSY), path),
            "The database is locked by another program. Try again when it's done."
        );
    }

    #[test]
    fn cancelled_and_wrong_files_read_like_the_app() {
        let path = Path::new("/x/a.sqlite");
        assert_eq!(map_statement_error(&failure(ffi::SQLITE_INTERRUPT), false), "canceling statement due to user request");
        assert_eq!(map_open_error(&failure(ffi::SQLITE_NOTADB), path), "/x/a.sqlite isn't a SQLite database.");
        assert_eq!(map_open_error(&failure(ffi::SQLITE_CANTOPEN), path), "The file /x/a.sqlite doesn't exist.");
    }

    #[test]
    fn statement_errors_about_files_keep_sqlites_text() {
        // An ATTACH of a text file or a temp file that can't be made isn't the session's file
        let not_a_db = failure_with(ffi::SQLITE_NOTADB, "file is not a database");
        assert_eq!(map_statement_error(&not_a_db, false), "file is not a database");
        let cant_open = failure_with(ffi::SQLITE_CANTOPEN, "unable to open database: /y/b.sqlite");
        assert_eq!(map_statement_error(&cant_open, false), "unable to open database: /y/b.sqlite");
    }

    #[test]
    fn attaching_a_text_file_does_not_blame_the_main_file() {
        let main = TempDb::with("CREATE TABLE t(x)");
        let text = TempDb::raw(b"hello, this is not a database at all, just some text...");
        let conn = rusqlite::Connection::open(main.path()).unwrap();
        let err = conn
            .execute("ATTACH ?1 AS other", [text.path().display().to_string()])
            .and_then(|_| conn.query_row("SELECT count(*) FROM other.sqlite_master", [], |r| r.get::<_, i64>(0)).map(|_| 0))
            .err()
            .unwrap();
        let message = map_statement_error(&err, false);
        assert_eq!(message, err.to_string());
        assert!(!message.contains(&main.path().display().to_string()), "{message}");
    }

    #[test]
    fn a_lock_inside_one_connection_keeps_sqlites_text() {
        let locked = failure_with(ffi::SQLITE_LOCKED, "database table is locked");
        assert_eq!(map_statement_error(&locked, false), "database table is locked");
        assert_eq!(map_statement_error(&locked, true), "database table is locked");

        // DROP while a SELECT on the same connection is still reading
        let f = TempDb::with("CREATE TABLE t(x); INSERT INTO t VALUES (1), (2);");
        let conn = rusqlite::Connection::open(f.path()).unwrap();
        let mut reading = conn.prepare("SELECT x FROM t").unwrap();
        let mut rows = reading.query([]).unwrap();
        rows.next().unwrap();
        let err = conn.execute_batch("DROP TABLE t").err().unwrap();
        let message = map_statement_error(&err, false);
        assert_ne!(message, "The database is locked by another program. Try again when it's done.");
        assert_eq!(message, err.to_string());
    }

    #[test]
    fn result_codes_have_their_names() {
        assert_eq!(code_name(ffi::SQLITE_CONSTRAINT_NOTNULL), "SQLITE_CONSTRAINT_NOTNULL");
        assert_eq!(code_name(ffi::SQLITE_ERROR), "SQLITE_ERROR");
        assert_eq!(code_name(ffi::SQLITE_INTERRUPT), "SQLITE_INTERRUPT");
        assert_eq!(code_name(ffi::SQLITE_BUSY_TIMEOUT), "SQLITE_BUSY_TIMEOUT");
        // An extended code this version doesn't know gets its primary code's name
        assert_eq!(code_name(ffi::SQLITE_IOERR | (99 << 8)), "SQLITE_IOERR");
        assert_eq!(code_name(255), "SQLITE_ERROR");
    }

    #[test]
    fn sqlite_permission_errors_are_not_file_problems() {
        let path = Path::new("/x/a.sqlite");
        let perm = failure_with(ffi::SQLITE_PERM, "access permission denied");
        assert_eq!(map_open_error(&perm, path), "access permission denied");
        assert_eq!(map_statement_error(&perm, false), "access permission denied");
    }
}
