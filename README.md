<p align="center">
  <img src="web/public/icon.svg" width="120" alt="DBunny logo" />
</p>

<h1 align="center">DBunny</h1>

<p align="center">A fast, lightweight desktop client for PostgreSQL.</p>

<p align="center">
  <a href="https://github.com/coelhomarcus/dbunny/releases/latest"><img src="https://img.shields.io/github/v/release/coelhomarcus/dbunny?sort=semver&label=&logo=github&labelColor=gray&color=gray" alt="Latest Release" /></a>
  <a href="https://github.com/coelhomarcus/dbunny/stargazers"><img src="https://img.shields.io/github/stars/coelhomarcus/dbunny?label=&logo=github&labelColor=gray&color=gray" alt="Stars" /></a>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/user-attachments/assets/7e2436b2-2b8c-4cc9-a2ac-d9e1660150d6" />
    <img src="https://github.com/user-attachments/assets/a237fc1c-56da-464e-921f-b4516e98f26d" width="720" alt="DBunny showing a table's rows, with the explorer on the left" />
  </picture>
</p>

## About

DBunny is a native PostgreSQL client for **macOS**, **Windows** and **Linux**, built with Tauri and React. It covers the everyday work with a database (connecting, browsing, editing rows, running queries, changing structure) in a small, quick app.

## Features

- **Connections:** saved connections with names and colors, one tab per connection, SSL with certificates, SSH tunnels, Unix sockets, read-only mode, and credentials encrypted at rest.
- **Explorer:** schemas, tables, views, materialized views, partitions, functions, sequences and types, with a filter, pinned tables and columns inline.
- **Data:** paginated and sortable tables, filters, inline editing with a review of the SQL before saving, insert, delete, CSV import, and export to CSV or JSON.
- **Queries:** a SQL editor with autocomplete, run the statement at the cursor or the whole script, EXPLAIN plans, transactions, formatting, history and saved queries. Safe mode asks before destructive statements.
- **Structure:** columns, indexes, constraints, foreign keys, triggers, permissions and DDL, all editable, with the SQL shown before it runs.
- **Server:** create and drop schemas and databases, see roles and live activity, and cancel or end sessions.

## Getting started

Download the app for your system from the [releases page](https://github.com/coelhomarcus/dbunny/releases/latest):

| Your system | Download |
|---|---|
| macOS on Apple Silicon (M1 and later) | `DBunny-<version>-macOS-Apple-Silicon.dmg` |
| macOS on Intel | `DBunny-<version>-macOS-Intel.dmg` |
| Windows | `DBunny-<version>-Windows-x64-setup.exe` |
| Linux | `DBunny-<version>-Linux-x64.AppImage`, or the `.deb` / `.rpm` |

To build it yourself you need [Node.js](https://nodejs.org) and [Rust](https://www.rust-lang.org/tools/install):

```bash
npm install
npm run dev     # run in development
npm run build   # build the installer
```

## Tech stack

React 19, TypeScript, Tailwind CSS 4 and CodeMirror on the front; Tauri 2 and Rust with tokio-postgres on the back.

## Credits

Logo: [SVG Repo, Bunny](https://www.svgrepo.com/svg/452876/bunny)
