# Databases in DBunny

How DBunny talks to each database it supports: which versions, which libraries, and how every feature is carried out, step by step. It describes the code as of version 0.13.0.

- [How the code is organised](#how-the-code-is-organised)
- [Versions](#versions)
- [PostgreSQL](#postgresql)
- [MySQL and MariaDB](#mysql-and-mariadb)
- [SQLite](#sqlite)
- [What every server shares](#what-every-server-shares)
- [The assistant](#the-assistant)
- [Libraries and repositories](#libraries-and-repositories)
- [Testing](#testing)

## How the code is organised

The backend is Rust (Tauri 2) under `src-tauri/src/db/`; the interface is React under `web/`.

| Piece | Where | What it does |
|---|---|---|
| Driver contract | `db/driver.rs` | `Driver` (a session), `Catalog` (what the database holds) and `Data` (a table's rows) are the core every engine implements. `Console`, `Transactions`, `SchemaEditor`, `ServerAdmin` and `ReadOnlyQueries` are optional: a driver hands them out only when its engine has them. |
| Capabilities | `db/capabilities.rs` | What the connected engine and version support, sent to the interface on connect. Screens and actions it lacks are hidden; the interface never compares versions itself. |
| Version | `db/version.rs` | `ServerVersion`, the end-of-support table and the tests for servers that only look like PostgreSQL or MySQL. |
| Shared SQL | `db/sql/` | The `Dialect` (quoting, parameters, limits), the builders of `WHERE`, `ORDER BY`, `INSERT`/`UPDATE`/`DELETE`, the script splitter and the check that a statement only reads. Nothing a person types becomes SQL text in these builders: every value is a parameter. |
| Network | `db/net/` | The SSH tunnel and the TLS certificate settings every server driver uses. |
| Engines | `db/postgres/`, `db/mysql_common/` (+ `db/mysql/`, `db/mariadb/`), `db/sqlite/` | One module per engine. MySQL and MariaDB share one driver, `ServerDriver`, and differ through a `Flavor`. |
| Saved connections | `commands/storage.rs` | Connections are saved with their engine; passwords and other secrets are encrypted with AES-256-GCM (`aes-gcm`). |

Errors DBunny makes itself are coded (`AppError::coded`) and translated; what a database server says passes through as it came.

The SQL editor's autocomplete, with its dialect per database, is described in [AUTOCOMPLETE.md](AUTOCOMPLETE.md).

## Versions

**Supported** means the versions their vendors still maintain. Every one of them is tested on each release, TLS and SSH tunnels included ([Testing](#testing)). A version past its end of support keeps working: DBunny says so once and keeps a mark in the status bar. Below a floor, the connection is refused.

| Database | Supported and tested | Works, past its end of support | Refused |
|---|---|---|---|
| PostgreSQL | 14, 15, 16, 17, 18 | 13 | 12 and older |
| MySQL | 8.4, 9.7, 26.10 | 8.0 | 5.7 and older |
| MariaDB | 10.11, 11.4, 11.8, 12.3, 13.0 | 10.6 | 10.5 and older |
| SQLite | any database file; SQLite 3.53 is bundled | | |

PostgreSQL 14 leaves its vendor's support on 12 November 2026.

### End of support

`db/version.rs` holds every version line since the oldest DBunny opens, with its last day of support (from [endoflife.date](https://endoflife.date)), short-lived lines included: MySQL Innovation releases (9.0–9.6, 26.7) and MariaDB rolling releases (10.7–10.10, 11.0–11.3, 11.5–11.7, 12.0–12.2, 13.0).

1. On connect, the driver gives its `ServerVersion` (`Driver::version`).
2. `past_end_of_support(engine, version, today)` finds the line: the major version for PostgreSQL, major.minor for MySQL and MariaDB. A version newer than the table has no end of support yet; one the table doesn't list takes the date of the line before it.
3. When today (the local date) is past that day, the answer to the connection carries `endOfSupport: { line, date }`.
4. The interface shows a notice above the tabs the first time, remembered per saved connection (or engine, host and port) and version line in the browser's local storage, and keeps a warning mark next to the version in the status bar.

### Servers that only look like PostgreSQL or MySQL

Some products speak the PostgreSQL or MySQL protocol and report the version they emulate, not their own release. For them, DBunny treats the version as unknown: no floor, no end-of-support notice, and the capabilities every supported version has.

- PostgreSQL side (`postgres_is_stock`): `version()` must start with `PostgreSQL ` and `server_version` must be a plain number. YugabyteDB (`11.2-YB-…`), Greenplum, CockroachDB and Redshift don't qualify.
- MySQL side (`mysql_is_stock`): TiDB and Vitess (PlanetScale) write their name into `VERSION()`; Aurora MySQL answers `SELECT @@aurora_version`.

### What changes with the version

| Capability | PostgreSQL | MySQL | MariaDB |
|---|---|---|---|
| `EXPLAIN ANALYZE` | all | 8.0.18 | all (as `ANALYZE`) |
| `MAINTAIN` table privilege | 17 | — | — |
| `VIRTUAL` generated columns | 18 | all | all |
| CHECK constraints | all | 8.0.16 | all |
| Index on an expression (read) | all | 8.0.13 | — (MariaDB indexes virtual columns instead) |

## PostgreSQL

**Library**: [`tokio-postgres`](https://github.com/sfackler/rust-postgres) 0.7, TLS through [`postgres-native-tls`](https://github.com/sfackler/rust-postgres) and [`native-tls`](https://github.com/sfackler/rust-native-tls) (the system's TLS: Security.framework on macOS, SChannel on Windows, OpenSSL on Linux).

### Connecting

1. The connection is a URL (`postgresql://user:password@host:port/database?…`), built from the form or pasted. A host starting with `/` is the folder of a Unix socket.
2. **TLS** follows libpq's `sslmode`: `disable`; `prefer` and `require` encrypt without checking the certificate (as libpq, `require` with a CA file checks the chain); `verify-ca` checks the chain; `verify-full` also checks the host name. A client certificate is a PEM file with a PKCS#8 key. On macOS the system rejects server certificates without `serverAuth` or valid for more than 825 days.
3. **SSH**: the database is reached through a local port forwarded over an SSH session ([SSH tunnels](#ssh-tunnels)).
4. The driver reads `server_version`, `server_version_num` and `version()`. Below 13 it refuses: "PostgreSQL 12.22 isn't supported; DBunny needs PostgreSQL 13 or newer." The capabilities follow the version.
5. **Read-only** connections add `options=-c default_transaction_read_only=on`; a read replica shows as read-only too (`transaction_read_only`).

A session has a main connection (explorer, tables, structure, administration), the query editor's own connection, a transaction connection when the editor opens one, and the assistant's reader. A lost connection is opened again on the next command.

### Explorer and catalog

Everything is read from `pg_catalog` (`pg_class`, `pg_namespace`, `pg_attribute`, `pg_constraint`, `pg_proc`, `pg_type`, `pg_index`, `pg_trigger`, `pg_sequence`…). Schemas other than the system ones are listed with their tables, views, materialized views, partitioned tables, functions, procedures, sequences and types. Definitions come from `pg_get_viewdef`, `pg_get_functiondef`, `pg_get_triggerdef` and similar; a table's DDL is rebuilt from the catalog, since PostgreSQL has no single function for it. From PostgreSQL 17 a domain's `NOT NULL` is also a constraint and from 18 a column's is too; both are left out where DBunny writes `NOT NULL` itself.

### Table data

1. A page is `SELECT … FROM table WHERE … ORDER BY … LIMIT … OFFSET …`, with filters and values as parameters (PostgreSQL gets every value as text and converts it to the column's type).
2. The row count is `COUNT(*)`, unless the planner's estimate (`pg_class.reltuples`) is 100,000 or more, which is then shown as an estimate.
3. Inserting, saving and deleting use the shared builders; saving many rows runs in one transaction, so all of it is saved or none is.
4. Types the binary protocol can't decode are read as text (`db/postgres/text.rs`).

### SQL editor

1. A script is split into statements (`db/sql/split.rs`), aware of strings, quoted names, comments and dollar-quoted bodies (`$$ … $$`).
2. Statements run one at a time on the editor's connection; the result of each is kept, and an error names the statement and line.
3. Cancel sends PostgreSQL's cancel request (`cancel_token`).
4. EXPLAIN is `EXPLAIN (FORMAT JSON)`; with analyze, `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` inside a transaction that is rolled back.
5. The Transaction button keeps a transaction open across runs, on a connection of its own, until commit or rollback.

### Structure and administration

Columns, indexes (including `CONCURRENTLY`), constraints, foreign keys, triggers (enable and disable), comments and permissions are changed with statements the app shows before running them. Types, defaults and expressions are SQL the person writes, checked to be a single fragment. Tables can be renamed, duplicated, truncated (with `RESTART IDENTITY` and `CASCADE`) and dropped; materialized views refreshed. The server screens list databases (create, drop with `WITH (FORCE)` to end other sessions), schemas, roles and memberships (`pg_roles`, `pg_auth_members`), and activity (`pg_stat_activity`, `pg_blocking_pids`), where a session's query can be cancelled or the session ended.

## MySQL and MariaDB

**Library**: [`mysql_async`](https://github.com/blackbeam/mysql_async) 0.37.1 with rustls (`default-rustls-ring`) and `client_ed25519` (MariaDB's ed25519 logins).

MySQL and MariaDB are separate engines with one shared driver (`db/mysql_common/`). Each has a `Flavor` (`db/mysql/flavor.rs`, `db/mariadb/flavor.rs`) for what differs: version rules, capabilities, system databases, how plans and accounts are read. A database plays the part of a schema.

### Connecting

1. The form has host, port, database (optional: without one, the first database the user can see opens), user and password, or a `mysql://` URL. A host starting with `/` is a Unix socket.
2. The driver reads `VERSION()`. MySQL below 8.0 and MariaDB below 10.6 are refused, and each driver refuses the other server with a message saying to choose the other engine. Older MariaDB servers prefix the version with `5.5.5-`, which is removed.
3. **TLS**: `require` encrypts without checking the certificate; `verify-full` checks the chain and the host name; `verify-ca` checks the host name too (a limit of `mysql_async` with rustls). The certificates MySQL makes for itself name no host, so only `require` works with them.
4. **Read-only** connections run `SET SESSION TRANSACTION READ ONLY` on every connection; a server with `read_only` on (MariaDB 12 reports `OFF`, `ON`, `NO_LOCK` or `NO_LOCK_NO_ADMIN`) also marks the session read-only.
5. Every connection runs `SET NAMES utf8mb4` and `SET time_zone = '+00:00'`, and opens with `CLIENT_FOUND_ROWS`, so saving a value unchanged still counts the row.

A session has a pool of up to 4 connections (explorer, tables, structure, administration; with `innodb_lock_wait_timeout = 5`, so a save blocked by the editor's transaction fails in seconds) and the query editor's own connection, which also keeps its transaction.

### Explorer and catalog

Read from `information_schema` (`TABLES`, `COLUMNS`, `STATISTICS`, `TABLE_CONSTRAINTS`, `KEY_COLUMN_USAGE`, `CHECK_CONSTRAINTS`, `ROUTINES`, `TRIGGERS`, privileges) for the open database only; DDL from `SHOW CREATE TABLE/VIEW/PROCEDURE/FUNCTION/TRIGGER`. System databases (`mysql`, `sys`, `information_schema`, `performance_schema`) are hidden. An index the optimizer ignores is written `INVISIBLE` on MySQL and `IGNORED` on MariaDB.

### Table data

1. A page is `SELECT … LIMIT … OFFSET …`; the count is `COUNT(*)` unless `information_schema.TABLES.TABLE_ROWS` is 100,000 or more.
2. Saving and importing run in one `START TRANSACTION` on a pool connection. A table whose engine can't undo changes (MyISAM, MEMORY…) says how many rows were already saved when a later one fails.
3. Values are converted to the grid's shapes (`db/mysql_common/values.rs`): large integers and decimals as text, dates and times as text, JSON as JSON, binary as `\x…` hex, geometry as WKT (read-only).

### SQL editor

1. The splitter (`db/mysql_common/split.rs`, shared with the interface's table of cases) knows `#`, `-- ` and `/* */` comments, quoted strings and names, and procedure, function, trigger and event bodies (`BEGIN … END`, `IF`, `CASE`, `LOOP`…). A `DELIMITER` line is explained instead of run: it's a command of the `mysql` client.
2. Statements run one at a time on the editor's connection; `USE` changes the editor's database and survives a reconnection.
3. Cancel runs `KILL QUERY <id>` from a pool connection.
4. EXPLAIN: MySQL `EXPLAIN FORMAT=TREE` and `EXPLAIN ANALYZE` (8.0.18); MariaDB `EXPLAIN FORMAT=JSON` and `ANALYZE FORMAT=JSON`. Both become the same tree. Analyze runs only statements that read, since it executes them.
5. Transactions: `START TRANSACTION`, commit and rollback on the editor's connection; a statement that commits by itself (DDL) is reported.

### Structure and administration

Columns, indexes, constraints (CHECK, foreign keys) and permissions, with the app showing each statement before it runs; `MODIFY` keeps what `SHOW CREATE TABLE` had. Accounts and roles come from `mysql.user` and `mysql.role_edges` (MySQL) or `mysql.global_priv` and `mysql.roles_mapping` (MariaDB). Activity comes from `performance_schema` on MySQL and `information_schema.PROCESSLIST` on MariaDB (whose performance schema is off by default); a session's query is stopped with `KILL QUERY` and the session with `KILL CONNECTION`. Databases are listed, created and dropped.

## SQLite

**Library**: [`rusqlite`](https://github.com/rusqlite/rusqlite) 0.40.2 with `bundled` (SQLite 3.53 compiled into the app, so the system's SQLite doesn't matter).

### Opening

A session is one database file with two connections: one for the tables (explorer, data, structure) and one for the query editor and its transaction. `rusqlite` blocks, so each connection sits behind a mutex and runs on tokio's blocking threads. `PRAGMA foreign_keys` is on. The assistant reads on a third connection opened read-only. A "schema" is a database of the connection: `main`, `temp` or an attached one. Its version is the bundled library's; there is no floor and no end of support.

### What it does

- **Catalog**: `sqlite_master` and the pragmas (`table_xinfo`, `table_list`, `index_info`, `foreign_key_list`, `database_list`). `STRICT` and `WITHOUT ROWID` tables and generated columns are read.
- **Data**: every edit runs in its own `BEGIN IMMEDIATE` transaction on the tables' connection; values keep SQLite's dynamic types and are shown as the grid knows them from PostgreSQL.
- **Editor**: SQLite itself says where a statement ends (`sqlite3_complete`), so a semicolon inside a string or a trigger body doesn't cut it. Cancel interrupts the running statement. EXPLAIN is `EXPLAIN QUERY PLAN`. The editor's transaction is separate from the tables' connection, which sees the changes after the commit.
- **Structure**: adding, renaming and dropping columns, indexes and the table actions are done directly. A generated column can be added only as `VIRTUAL` (SQLite's rule). Changing a column's type, `NOT NULL`, default or constraints needs the table rebuilt, which DBunny doesn't do yet.

## What every server shares

### SSH tunnels

`db/net/ssh.rs`, with [`russh`](https://github.com/Eugeny/russh) 0.63: password or private key (ed25519, RSA), with or without a passphrase. The server's host key is checked against `~/.ssh/known_hosts` and the keys accepted in DBunny (`ssh_known_hosts.json` in the app's data folder). An unknown key shows its fingerprint to be confirmed; a changed key is refused, with a note when the old record is in `~/.ssh/known_hosts` (fixed there, not in DBunny). A local port on 127.0.0.1 forwards each database connection over the one SSH session. TLS can run inside the tunnel; for `verify-full` the certificate is checked against the database host, not the tunnel's.

### TLS files

`db/net/tls.rs` reads the CA, client certificate and key files given in the SSL tab (a path may start with `~`).

## The assistant

The assistant (Claude Code or Codex, run on the person's computer) works on the database through tools DBunny gives it over a local MCP server. It only reads.

1. **What it's told**: the engine, version and database, the tables (with columns when they fit), and the dialect: how to quote names and limit rows, and what the connected version lacks (for example `MERGE` and `NULLS NOT DISTINCT` before PostgreSQL 15, the SQL/JSON constructors and `IS JSON` before 16, `JSON_TABLE`, `JSON_EXISTS`, `JSON_QUERY` and `JSON_VALUE` before 17, `RETURNING` on MySQL, `LATERAL` before MySQL 8.0.14 and on MariaDB). Host, user and password are never in it.
2. **What it can run**: one statement that only reads (`db/sql/reading.rs` refuses writes, locks, several statements and functions whose effects outlive a transaction), on a connection apart from the editor's:
   - PostgreSQL: a read-only transaction with a `statement_timeout`, always rolled back, through a cursor that stops after the rows asked for;
   - MySQL and MariaDB: `START TRANSACTION READ ONLY` with `max_execution_time` (MySQL) or `max_statement_time` (MariaDB), always rolled back;
   - SQLite: a connection that can't write.
3. **Limits**: at most the configured number of rows (100 by default) and 30 seconds per statement.

## Libraries and repositories

| Library | Version | Used for | Repository |
|---|---|---|---|
| tokio-postgres | 0.7.16 | PostgreSQL protocol | [sfackler/rust-postgres](https://github.com/sfackler/rust-postgres) |
| postgres-native-tls, native-tls | 0.5.3, 0.2.18 | PostgreSQL TLS (system TLS) | [sfackler/rust-postgres](https://github.com/sfackler/rust-postgres), [sfackler/rust-native-tls](https://github.com/sfackler/rust-native-tls) |
| mysql_async | 0.37.1 | MySQL and MariaDB protocol | [blackbeam/mysql_async](https://github.com/blackbeam/mysql_async) |
| rustls | 0.23 | MySQL and MariaDB TLS | [rustls/rustls](https://github.com/rustls/rustls) |
| rusqlite | 0.40.2 (SQLite 3.53) | SQLite | [rusqlite/rusqlite](https://github.com/rusqlite/rusqlite) |
| russh | 0.63.3 | SSH tunnels | [Eugeny/russh](https://github.com/Eugeny/russh) |
| aes-gcm | 0.10 | Encrypting saved connections | [RustCrypto/AEADs](https://github.com/RustCrypto/AEADs) |
| chrono | 0.4 | Dates (values, end of support) | [chronotope/chrono](https://github.com/chronotope/chrono) |

Version dates come from [endoflife.date](https://endoflife.date). The test servers are the official Docker images [`postgres`](https://hub.docker.com/_/postgres), [`mysql`](https://hub.docker.com/_/mysql) and [`mariadb`](https://hub.docker.com/_/mariadb), with [`linuxserver/openssh-server`](https://hub.docker.com/r/linuxserver/openssh-server) for the tunnels.

## Testing

- **Unit tests** (`cargo test --manifest-path src-tauri/Cargo.toml --lib`, `npm test --workspace=web`) run on every push and pull request (`.github/workflows/test.yml`).
- **Server tests** are the `#[ignore]` tests that need a real server. `src-tauri/tests/servers.sh up <engine> <version>` starts one version in Docker (only `dbunny-test-*` containers, ports on 127.0.0.1) with TLS, SSH and the fixtures the tests expect, and prints the variables they read. `src-tauri/tests/matrix.sh` runs every version of the matrix one at a time and writes a summary to `src-tauri/target/matrix/summary.md`; `servers.sh down all` removes the containers with their volumes and `servers.sh prune` the images too.
- A test whose feature the version lacks returns early through `requires()` and prints a `dbunny-skip:` line, which the summary counts; skipping is never used for a failure. `check-filters.sh` makes sure every server test is run by some engine's filter.
- **CI** (`.github/workflows/servers.yml`) runs the same scripts with one job per version: weekly, on demand, and on pull requests to `main` that touch database or assistant code. A release pull request needs it green.

```bash
eval "$(src-tauri/tests/servers.sh up postgres 17)"
cargo test --manifest-path src-tauri/Cargo.toml --lib -- $(src-tauri/tests/servers.sh filters postgres) --ignored
src-tauri/tests/matrix.sh                 # all 16 versions
src-tauri/tests/servers.sh down all
```
