# AGENTS.md

Guidance for humans and coding agents working on DBunny. Follow these conventions unless the user asks for something different in the current task.

## Repository layout

- **`web/`** — React 19, TypeScript, Tailwind 4, CodeMirror (npm workspace).
- **`src-tauri/`** — Tauri 2, Rust; database logic under `src-tauri/src/db/` (driver contract, PostgreSQL, SQLite).
- **Root** — Tauri CLI scripts (`npm run dev`, `npm run build`).

## Branches

| Branch | Role |
| --- | --- |
| **`develop`** | Day-to-day work. Push feature and fix commits here. |
| **`main`** | Release line. Updated by merging `develop` via pull request. |

Do not commit directly to `main` unless the user explicitly requests it.

## Commits

### Format

```
type: short subject in lowercase english

- First bullet: what changed and, when useful, why
- More bullets for separate behaviors or edge cases
- Bullets are full sentences, lowercase start, no trailing period required but stay consistent within the commit
```

- **Subject:** one line, lowercase after `type: `, no period at the end. Be specific (prefer `fix: sqlite paths are always opened as plain absolute files` over `fix: sqlite bug`).
- **Body:** optional but common for non-trivial changes; use `-` bullets only (no numbered lists in commit messages).
- **Language:** English for subject and body.
- **Scope:** one logical change per commit when the user asks for separate commits; otherwise group what belongs together.

### Types (as used in this repo)

| Type | Use for |
| --- | --- |
| **`feat`** | New behavior or user-visible capability |
| **`fix`** | Bug fixes |
| **`improve`** | UX or polish without new features (still used in history) |
| **`refactor`** | Internal structure, no intended behavior change |
| **`chore`** | Version bumps, tooling, ignore files |
| **`ci`** | GitHub Actions and automation |
| **`docs`** | README, release notes text, AGENTS.md |
| **`test`** | Tests only |

Examples from this project:

```
feat: saved connections know their database engine
- saved connections store an engine; files without it load as postgres
- the encrypted payload is picked by engine, postgres keeps the current one
```

```
fix: the sqlite statement splitter closes a trigger the way sqlite does
- begin, case and end can be column names; only an END right after a semicolon closes a trigger body
```

### Version commits

When preparing a release on `develop` (before merging to `main`), bump the app version in:

- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/tauri.conf.json`

Use a dedicated commit:

```
chore: version X.Y.Z
```

(no body required)

### What to avoid in commits

- Do not add `Co-authored-by`, `Signed-off-by`, or other agent/tool trailers to commit messages.
- Do not mention other products or repos as inspiration (e.g. other database clients or tools).
- Do not push unless the user asks.
- Do not amend or rewrite published history unless the user asks.

## Pull requests

### When

Open a PR from **`develop`** → **`main`** when a release or a large milestone is ready, or when the user asks to merge.

### Title

- English, descriptive sentence or phrase.
- For release batches, include the version: `Driver contract, capabilities and SQLite support (0.5.0)`.

### Description

Structure PR bodies like recent merges on this repo:

1. **Opening paragraph** — What the PR does and who it affects (e.g. “Nothing changes for people who use PostgreSQL only”).
2. **`##` sections** — Group by theme (`## SQLite`, `## Driver contract`, `## Other`), not by file.
3. **`## Testing`** — What was actually run (Rust unit tests, frontend tests, typecheck, lint caveats). Be honest, e.g. “Not yet tested in the desktop app itself” when true.

Use bullets under sections for scanability. Match the tone of existing PRs: factual, complete, no emojis unless the user wants them.

### Merge

- Prefer a **merge commit** (not squash-only) when the user asks to merge `develop` into `main`, so history on `develop` stays intact on `main`.
- After merge, `main` carries the merge commit; tags for releases are created on `main`.

## Releases

Typical flow used on this project:

1. Finish work on **`develop`** (including `chore: version X.Y.Z` when appropriate).
2. Open and merge PR **`develop`** → **`main`**.
3. On **`main`**, create and push tag **`vX.Y.Z`** (must match `tauri.conf.json`).
4. Tag push runs **`.github/workflows/build.yml`**: multi-platform Tauri build, draft GitHub Release, `latest.json` for the in-app updater. A follow-up job removes standalone `.sig` assets from the release.
5. **Edit the draft release** on GitHub: replace the auto-generated body with a full changelog (highlights, per-area notes, upgrading notes, download table). Include the **macOS quarantine** note for DMG installs:

   ```bash
   xattr -dr com.apple.quarantine "/Applications/DBunny.app"
   ```

6. **Publish** the release when assets are ready.

Patch release notes and README in English. User-facing docs describe DBunny as a **multi-database** client; list supported engines accurately (see `README.md` and `web/src/lib/engines.ts`).

## CI

- **`test.yml`** — Runs on push to `develop` / `main` and on every PR: frontend unit tests, typecheck, Rust unit tests (PostgreSQL integration tests stay ignored in CI).
- Fix CI failures before merging when possible.

## Local checks (before claiming “done”)

When you change code, run what applies:

```bash
npm test --workspace=web
npx tsc -b web
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

For larger backend work, mention server tests (`cargo test` with PostgreSQL) only if the environment is available.

## Product conventions for agents

- **Capabilities:** UI and commands must respect per-engine `Capabilities` from the backend; do not show PostgreSQL-only actions for SQLite.
- **English** for README, release notes, PR text, and commit messages.
- **No `title=""` attributes** on UI elements unless the user wants them (project preference).
- **Encryption / saved connections:** treat paths and secrets carefully; match existing patterns in `commands/connect` and connection draft code.
- Ask before **force-push**, **deleting tags**, or **publishing releases** if the user has not already approved that step.

## Quick command reference

```bash
git checkout develop
npm run dev              # desktop dev
npm install && npm run build   # production installers locally
gh pr create --base main --head develop --title "..." --body-file ...
gh release edit vX.Y.Z --notes-file ...
```
