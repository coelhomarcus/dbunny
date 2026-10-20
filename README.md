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

| Database | Status | Versions | Connection |
| --- | --- | --- | --- |
| PostgreSQL | Supported | 13 and newer | Server (TCP, Unix socket, SSL, SSH tunnel) |
| SQLite | Supported | Any database file (SQLite 3.53 is bundled) | Local database file |
| MySQL | Supported | 8.0 and newer | Server (TCP, Unix socket, SSL, SSH tunnel) |
| MariaDB | Supported | 10.6 and newer | Server (TCP, Unix socket, SSL, SSH tunnel) |
| SQL Server | Planned | - | Server |

Screens and actions adapt to what the connected engine supports (for example, schemas and materialized views are PostgreSQL-only today, and MySQL and MariaDB have databases instead of schemas).

## Features

- **Connections:** saved connections with names and colors, one tab per connection; PostgreSQL, MySQL and MariaDB add SSL, SSH tunnels, Unix sockets and read-only mode; credentials and file paths encrypted at rest.
- **Explorer:** schemas, tables, views, indexes, triggers and more (exact objects depend on the engine), with a filter, pinned tables and inline columns.
- **Data:** paginated and sortable tables, filters, inline editing with a review of the SQL before saving, insert, delete, CSV import, and export to CSV or JSON.
- **Queries:** a SQL editor with autocomplete, run the statement at the cursor or the whole script, query plans, transactions, formatting, history and saved queries. Safe mode asks before destructive statements.
- **Structure:** columns, indexes, constraints, foreign keys, triggers, permissions and DDL where the engine allows it, with the SQL shown before it runs.
- **Assistant:** ask about your data, write SQL, explain, fix and optimize it, with the database's tables, columns and relationships in view (see below).
- **Languages:** English and Portuguese (Brazil). The app follows your system and can be set in Settings; the assistant answers in the same language.
- **Server (PostgreSQL, MySQL and MariaDB):** create and drop databases (and schemas on PostgreSQL), see roles or accounts and live activity, and cancel or end sessions.

## AI Assistant

An assistant for the connected database, in a side panel (or a tab of its own) next to your queries. Ask about your data in plain language and it explores the schema, runs queries and checks its answers against the real rows. From the query editor, ask it to explain a statement, find why one failed, or look at a statement's plan and suggest an index or a rewrite.

- **It runs Claude Code or Codex on your computer, with your own account.** Pick the agent in Settings. DBunny keeps no API key. You need [Node.js](https://nodejs.org) 20 or newer. Claude Code itself doesn't have to be installed: when you aren't signed in, the panel shows a command to sign in with the one that comes with the adapter (or, if you already use Claude Code, `claude` then `/login` works too). Codex runs with a folder and a sign-in of its own in DBunny, so your own Codex tools and login stay out of it; the panel opens a browser to sign in with ChatGPT the first time. Cursor isn't supported yet.
- **It only reads.** Each query runs in a read-only transaction that is rolled back, and waits for your approval (once, or for the rest of the conversation). SQL that changes data is shown for you to open in the editor and run yourself.
- **You decide what it sees.** It is told the engine, version and schema, never the host or the password. It sees the rows of the queries you allow, cut to a limit you set; for a saved connection you can turn on *Schema only* so it never sees a value.
- **Conversations are kept per saved connection** by the agent (Claude Code in `~/.claude/projects`, Codex in DBunny's own Codex folder), and can be reopened from the panel.

Turn it on from the assistant button in the tab bar (Cmd/Ctrl+I) after reading what it sends. Settings has a section for it.

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

React 19, TypeScript, Tailwind CSS 4 and CodeMirror on the front; Tauri 2 and Rust on the back, with a shared driver layer and engine-specific backends (PostgreSQL via tokio-postgres, MySQL and MariaDB via mysql_async, SQLite via bundled rusqlite).

## Credits

Logo: [SVG Repo, Bunny](https://www.svgrepo.com/svg/452876/bunny)
