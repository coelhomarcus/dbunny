<p align="center">
  <img src="web/public/icon.svg" width="120" alt="DBunny logo" />
</p>

<h1 align="center">DBunny</h1>

<p align="center">A fast, lightweight desktop client for relational databases.</p>

<p align="center">
  <a href="https://github.com/coelhomarcus/dbunny/releases/latest"><img src="https://img.shields.io/github/v/release/coelhomarcus/dbunny?sort=semver&label=&logo=github&labelColor=gray&color=gray" alt="Latest Release" /></a>
  <a href="https://github.com/coelhomarcus/dbunny/stargazers"><img src="https://img.shields.io/github/stars/coelhomarcus/dbunny?label=&logo=github&labelColor=gray&color=gray" alt="Stars" /></a>
</p>

<p align="center">
  <img src="https://github.com/user-attachments/assets/f7cf97d2-0654-48e7-bf6d-76c8172c57f4" width="720" alt="DBunny showing a table's rows, with the explorer on the left" />
</p>

## About

DBunny is a native database client for **macOS**, **Windows** and **Linux**, built with Tauri and React. Connect to a server or open a file, then browse schemas, edit rows, run queries and change structure in a small, quick app. The backend is built around pluggable drivers so new engines can be added over time; relational databases come first, with room to grow beyond SQL later.

## Supported databases

| Database | Status | Connection |
| --- | --- | --- |
| PostgreSQL | Supported | Server (TCP, Unix socket, SSL, SSH tunnel) |
| SQLite | Supported | Local database file |
| MySQL / MariaDB | Planned | Server |
| SQL Server | Planned | Server |

Screens and actions adapt to what the connected engine supports (for example, server administration and materialized views are PostgreSQL-only today).

## Features

- **Connections:** saved connections with names and colors, one tab per connection; PostgreSQL adds SSL, SSH tunnels, Unix sockets and read-only mode; credentials and file paths encrypted at rest.
- **Explorer:** schemas, tables, views, indexes, triggers and more (exact objects depend on the engine), with a filter, pinned tables and inline columns.
- **Data:** paginated and sortable tables, filters, inline editing with a review of the SQL before saving, insert, delete, CSV import, and export to CSV or JSON.
- **Queries:** a SQL editor with autocomplete, run the statement at the cursor or the whole script, query plans, transactions, formatting, history and saved queries. Safe mode asks before destructive statements.
- **Structure:** columns, indexes, constraints, foreign keys, triggers, permissions and DDL where the engine allows it, with the SQL shown before it runs.
- **Server (PostgreSQL):** create and drop schemas and databases, see roles and live activity, and cancel or end sessions.

## Getting started

Download the app for your system from the [releases page](https://github.com/coelhomarcus/dbunny/releases/latest):

| Your system | Download |
| --- | --- |
| macOS on Apple Silicon (M1 and later) | `DBunny-<version>-macOS-Apple-Silicon.dmg` |
| macOS on Intel | `DBunny-<version>-macOS-Intel.dmg` |
| Windows | `DBunny-<version>-Windows-x64-setup.exe` |
| Linux | `DBunny-<version>-Linux-x64.AppImage`, or the `.deb` / `.rpm` |

**macOS:** If you download the DMG directly from Releases instead of using Homebrew, macOS will quarantine it and you'll need to clear the flag once yourself:

```bash
xattr -dr com.apple.quarantine "/Applications/DBunny.app"
```

To build it yourself you need [Node.js](https://nodejs.org) and [Rust](https://www.rust-lang.org/tools/install):

```bash
npm install
npm run dev     # run in development
npm run build   # build the installer
```

## Tech stack

React 19, TypeScript, Tailwind CSS 4 and CodeMirror on the front; Tauri 2 and Rust on the back, with a shared driver layer and engine-specific backends (PostgreSQL via tokio-postgres, SQLite via bundled rusqlite).

## Credits

Logo: [SVG Repo, Bunny](https://www.svgrepo.com/svg/452876/bunny)
