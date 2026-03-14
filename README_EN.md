<p align="center">
  <img src="web/public/icon.svg" width="120" alt="DBunny logo" />
</p>

<h1 align="center">DBunny</h1>

<p align="center">A fast, lightweight desktop client for PostgreSQL databases.</p>

<p align="center">
  <a href="https://github.com/coelhomarcus/dbunny/releases/latest"><img src="https://img.shields.io/github/v/release/coelhomarcus/dbunny?sort=semver&label=&logo=github&labelColor=gray&color=gray" alt="Latest Release" /></a>
  <a href="https://github.com/coelhomarcus/dbunny/stargazers"><img src="https://img.shields.io/github/stars/coelhomarcus/dbunny?label=&logo=github&labelColor=gray&color=gray" alt="Stars" /></a>
</p>

---

<p align="center">
  <img src="https://marcuscoelho.com/assets/projects/dbunny.webp" width="720" alt="DBunny screenshot" />
</p>

## About

DBunny is a desktop PostgreSQL client built with Tauri and React. It provides a clean, dark-themed interface for managing connections, browsing schemas, editing data, and running queries — all from a native desktop application.

Available for **Windows**, **macOS** (Intel & Apple Silicon), and **Linux**.

## Features

### Connection Management
- Save, edit, and organize multiple connections with custom names and colors
- Import connections from a PostgreSQL URL (`postgresql://user:pass@host:5432/db`)
- SSL/TLS support
- Encrypted credential storage (AES-GCM with PBKDF2 key derivation)
- Connection details masked by default for privacy

### Query Editor
- Multi-tab SQL editor powered by Monaco Editor with syntax highlighting
- Execute queries with `Ctrl+Enter` (run selected text or full editor content)
- SQL autocomplete with 40+ keywords
- Safe Mode enabled by default — detects and blocks destructive queries before execution:
  - `DROP DATABASE`, `DROP SCHEMA`, `DROP TABLE`
  - `TRUNCATE`, `DELETE` without `WHERE`, `UPDATE` without `WHERE`
  - `ALTER TABLE DROP COLUMN`
- Import and export `.sql` files
- Query tabs persist across navigation

### Data Browsing & Editing
- Browse table data with server-side pagination and sorting
- Inline cell editing with pending changes tracking
- Batch row deletion with multi-select
- Visual diff highlighting for modified cells
- Resizable columns
- Keyboard shortcuts: `Ctrl+S` to save, `Escape` to cancel

### Schema Explorer
- Hierarchical tree view of schemas, tables, views, and functions
- Lazy-loaded folders for fast navigation
- View table structure (columns, types, nullability, defaults, primary keys)
- Inspect PostgreSQL functions (source code, language, volatility, arguments)

## Tech Stack

| Layer | Technologies |
|-------|-------------|
| Frontend | React 19, TypeScript, Tailwind CSS 4, Monaco Editor, TanStack Table |
| Backend | Tauri 2 (Rust), tokio-postgres, deadpool-postgres |
| Security | AES-GCM, PBKDF2, SHA-256 |
| Build | Vite, GitHub Actions (multi-platform CI/CD) |

## Getting Started

**Prerequisites:** Node.js, npm, and [Rust](https://www.rust-lang.org/tools/install)

```bash
# install dependencies
npm install

# run in dev mode
npm run dev
```

To build for production:

```bash
npm run build
```

Or download a pre-built binary from the [releases page](https://github.com/coelhomarcus/dbunny/releases/latest).

## Credits

- Logo: [SVG Repo — Bunny](https://www.svgrepo.com/svg/452876/bunny)
