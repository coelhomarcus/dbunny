//! The SQLite driver: a session on one database file, with two connections, one for
//! the tables and one for the query editor (and its transaction).

mod conn;
pub mod errors;
#[cfg(test)]
mod tests_support;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

pub(crate) use conn::SqliteConn;

use super::model::{
    ColumnInfo, CompletionSchema, ExplorerTree, ExtensionInfo, FunctionDetail, JsonMap, ObjectDefinition, RowChange,
    TableDataParams, TableDataResponse, TableStructure,
};
use super::{Capabilities, Catalog, ConnectionInfo, Data, Driver, Engine};

/// How a file is opened.
pub struct OpenOptions {
    /// Open the file for reading only.
    pub read_only: bool,
    /// Create the file when it doesn't exist (only when the user asked for a new database).
    pub create: bool,
    /// How long a statement waits for a lock another connection holds before failing.
    pub busy_timeout: Duration,
}

impl Default for OpenOptions {
    fn default() -> Self {
        OpenOptions { read_only: false, create: false, busy_timeout: Duration::from_secs(5) }
    }
}

/// One open session on a SQLite file.
pub struct SqliteDriver {
    /// Reads and edits the tables.
    tables: SqliteConn,
    /// Runs the query editor and keeps its transaction.
    editor: SqliteConn,
    info: ConnectionInfo,
    capabilities: Capabilities,
}

impl SqliteDriver {
    /// Opens `path` as a new session. Nothing is created unless `opts.create` asks for it.
    pub async fn open(path: PathBuf, opts: OpenOptions) -> Result<Arc<SqliteDriver>, String> {
        tokio::task::spawn_blocking(move || {
            // Always an absolute path: a relative "file:..." name would be read as a URI
            let path = std::path::absolute(&path).unwrap_or(path);
            let tables = SqliteConn::open(&path, &opts)?;
            let editor = SqliteConn::open(&path, &opts)?;
            let info = ConnectionInfo {
                database: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                host: path.display().to_string(),
                port: 0,
                server_version: rusqlite::version().to_string(),
                read_only: tables.read_only() || editor.read_only(),
            };
            Ok(Arc::new(SqliteDriver { tables, editor, info, capabilities: Capabilities::sqlite() }))
        })
        .await
        .map_err(|e| format!("Opening the file stopped unexpectedly: {e}"))?
    }

    /// The connection that reads and edits the tables.
    #[allow(dead_code)] // Used once the SQLite catalog and data land
    pub(crate) fn tables(&self) -> &SqliteConn {
        &self.tables
    }

    /// The connection of the query editor and its transaction.
    #[allow(dead_code)] // Used once the SQLite query editor lands
    pub(crate) fn editor(&self) -> &SqliteConn {
        &self.editor
    }
}

#[async_trait]
impl Driver for SqliteDriver {
    fn engine(&self) -> Engine {
        Engine::Sqlite
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn info(&self) -> ConnectionInfo {
        self.info.clone()
    }

    /// Lets go of both connections, which closes them. A statement already running on one
    /// ends on its own first; it isn't waited for. Anything the session is asked
    /// afterwards answers "Session not found".
    async fn close(&self) {
        self.tables.close();
        self.editor.close();
    }

    fn catalog(&self) -> &dyn Catalog {
        &NotReady
    }

    fn data(&self) -> &dyn Data {
        &NotReady
    }
}

/// TEMPORARY: the catalog and the data of a SQLite session answer an error until the
/// SQLite catalog (Task 3) and data (Task 4) replace this stand-in.
struct NotReady;

const CATALOG_NOT_READY: &str = "The SQLite catalog isn't ready yet";
const DATA_NOT_READY: &str = "Table data isn't ready yet for SQLite";

#[async_trait]
impl Catalog for NotReady {
    async fn explorer(&self) -> Result<ExplorerTree, String> {
        Err(CATALOG_NOT_READY.into())
    }
    async fn columns(&self, _: &str, _: &str) -> Result<Vec<ColumnInfo>, String> {
        Err(CATALOG_NOT_READY.into())
    }
    async fn relation_kind(&self, _: &str, _: &str) -> Result<Option<String>, String> {
        Err(CATALOG_NOT_READY.into())
    }
    async fn table_structure(&self, _: &str, _: &str) -> Result<TableStructure, String> {
        Err(CATALOG_NOT_READY.into())
    }
    async fn completion_schema(&self) -> Result<CompletionSchema, String> {
        Err(CATALOG_NOT_READY.into())
    }
    async fn object_definition(&self, _: &str, _: &str, _: &str) -> Result<ObjectDefinition, String> {
        Err(CATALOG_NOT_READY.into())
    }
    async fn function_detail(&self, _: &str, _: &str, _: &str) -> Result<FunctionDetail, String> {
        Err(CATALOG_NOT_READY.into())
    }
    async fn extensions(&self) -> Result<Vec<ExtensionInfo>, String> {
        Err(CATALOG_NOT_READY.into())
    }
}

#[async_trait]
impl Data for NotReady {
    async fn table_page(&self, _: &str, _: &str, _: TableDataParams) -> Result<TableDataResponse, String> {
        Err(DATA_NOT_READY.into())
    }
    async fn count_rows(&self, _: &str, _: &str) -> Result<i64, String> {
        Err(DATA_NOT_READY.into())
    }
    async fn insert_row(&self, _: &str, _: &str, _: JsonMap) -> Result<u64, String> {
        Err(DATA_NOT_READY.into())
    }
    async fn save_rows(&self, _: &str, _: &str, _: Vec<RowChange>) -> Result<u64, String> {
        Err(DATA_NOT_READY.into())
    }
    async fn delete_rows(&self, _: &str, _: &str, _: Vec<JsonMap>) -> Result<u64, String> {
        Err(DATA_NOT_READY.into())
    }
    async fn import_rows(&self, _: &str, _: &str, _: Vec<String>, _: Vec<Vec<Option<String>>>) -> Result<u64, String> {
        Err(DATA_NOT_READY.into())
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::TempDb;
    use super::*;

    #[tokio::test]
    async fn the_stand_in_catalog_and_data_answer_errors() {
        let f = TempDb::with("CREATE TABLE t(x)");
        let d = SqliteDriver::open(f.path(), OpenOptions::default()).await.unwrap();
        assert_eq!(d.engine(), Engine::Sqlite);
        assert_eq!(d.capabilities(), &Capabilities::sqlite());
        assert!(d.catalog().explorer().await.is_err());
        assert!(d.data().count_rows("main", "t").await.is_err());
        assert!(d.console().is_none() && d.admin().is_none());
    }

    /// Removes a file when it goes out of scope (also if an assertion fails first).
    struct RemoveOnDrop(PathBuf);
    impl Drop for RemoveOnDrop {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[tokio::test]
    async fn a_name_that_looks_like_a_uri_is_a_plain_file_name() {
        // The bundled SQLite turns URI names on for every connection, so a relative name such as
        // "file:x?mode=memory" would open an in-memory database instead of a file
        let name = format!("file:dbunny-test-{}.db?mode=memory", uuid::Uuid::new_v4());
        let on_disk = std::env::current_dir().unwrap().join(&name);
        let _cleanup = RemoveOnDrop(on_disk.clone());
        let d = SqliteDriver::open(PathBuf::from(&name), OpenOptions { create: true, ..Default::default() })
            .await
            .unwrap();
        let file: String = d
            .tables()
            .run(|c| {
                c.query_row("SELECT file FROM pragma_database_list WHERE name = 'main'", [], |row| row.get(0))
                    .map_err(|e| e.to_string())
            })
            .await
            .unwrap();
        assert!(!file.is_empty(), "the database is a file, not an in-memory one");
        assert_eq!(d.info().host, on_disk.display().to_string(), "the session shows the absolute path");
    }

    #[tokio::test]
    async fn opens_an_existing_database() {
        let f = TempDb::with("CREATE TABLE t(id INTEGER PRIMARY KEY)");
        let d = SqliteDriver::open(f.path(), OpenOptions::default()).await.unwrap();
        assert_eq!(d.info().database, f.file_name());
        assert_eq!(d.info().host, f.path().display().to_string());
        assert_eq!(d.info().port, 0);
        assert_eq!(d.info().server_version, rusqlite::version());
        assert!(!d.info().read_only);
        assert!(rusqlite::version_number() >= 3_035_000);
    }

    #[tokio::test]
    async fn a_missing_file_is_not_created_unless_asked() {
        let missing = TempDb::missing();
        let path = missing.path();
        let err = SqliteDriver::open(path.clone(), OpenOptions::default()).await.err().unwrap();
        assert_eq!(err, format!("The file {} doesn't exist.", path.display()));
        assert!(!path.exists());
        SqliteDriver::open(path.clone(), OpenOptions { create: true, ..Default::default() }).await.unwrap();
        assert!(path.exists());
    }

    #[tokio::test]
    async fn text_files_and_folders_are_refused_with_a_clear_message() {
        let text = TempDb::raw(b"hello, this is not a database at all, just some text...");
        let err = SqliteDriver::open(text.path(), OpenOptions::default()).await.err().unwrap();
        assert_eq!(err, format!("{} isn't a SQLite database.", text.path().display()));
        let dir = std::env::temp_dir();
        assert!(SqliteDriver::open(dir, OpenOptions::default()).await.is_err());
    }

    #[tokio::test]
    async fn read_only_refuses_writes() {
        let f = TempDb::with("CREATE TABLE t(x)");
        let d = SqliteDriver::open(f.path(), OpenOptions { read_only: true, ..Default::default() }).await.unwrap();
        assert!(d.info().read_only);
        let err = d.tables().run(|c| c.execute("INSERT INTO t VALUES (1)", []).map_err(|e| e.to_string())).await.err().unwrap();
        assert!(err.contains("readonly"), "{err}");
    }

    #[tokio::test]
    async fn an_empty_file_opens_as_an_empty_database_and_stays_untouched() {
        let f = TempDb::raw(b"");
        let d = SqliteDriver::open(f.path(), OpenOptions::default()).await.unwrap();
        assert!(!d.info().read_only);
        let tables = d.tables().run(|c| c.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string())).await.unwrap();
        assert_eq!(tables, 0);
        assert_eq!(std::fs::metadata(f.path()).unwrap().len(), 0);
    }

    #[tokio::test]
    async fn the_editor_and_the_tables_use_different_connections() {
        let f = TempDb::with("CREATE TABLE t(x)");
        let d = SqliteDriver::open(f.path(), OpenOptions::default()).await.unwrap();
        d.editor().run(|c| c.execute_batch("CREATE TEMP TABLE only_here(x)").map_err(|e| e.to_string())).await.unwrap();
        let count = |name: &'static str| {
            move |c: &mut rusqlite::Connection| {
                c.query_row("SELECT count(*) FROM temp.sqlite_master WHERE name = ?1", [name], |r| r.get::<_, i64>(0))
                    .map_err(|e| e.to_string())
            }
        };
        assert_eq!(d.editor().run(count("only_here")).await.unwrap(), 1);
        assert_eq!(d.tables().run(count("only_here")).await.unwrap(), 0);
        let fks = |c: &mut rusqlite::Connection| c.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string());
        assert_eq!(d.tables().run(fks).await.unwrap(), 1);
        assert_eq!(d.editor().run(fks).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn a_closed_session_answers_session_not_found() {
        let f = TempDb::with("CREATE TABLE t(x)");
        let d = SqliteDriver::open(f.path(), OpenOptions::default()).await.unwrap();
        d.close().await;
        let err = d.tables().run(|_| Ok(())).await.err().unwrap();
        assert_eq!(err, "Session not found");
        let err = d.editor().run(|_| Ok(())).await.err().unwrap();
        assert_eq!(err, "Session not found");
    }
}
