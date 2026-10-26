# AGENTS.md

Guidance for humans and coding agents working on DBunny. Follow these conventions unless the user asks for something different in the current task.

## No co-authorship

Git history and GitHub metadata on this repo should **not** credit agents, IDEs, or other tools as co-authors. Do not add attribution unless the user explicitly asks for it.

| Where | Rule |
| --- | --- |
| **Commits** | No `Co-authored-by`, `Signed-off-by`, `Assisted-by`, or similar trailers in the message. |
| **Pull requests** | No co-author list, no “written with …”, no agent credits in the title, description, or PR comments. |
| **Releases** | No tool or agent attribution in release notes or changelog text edited on GitHub. |
| **Issues & reviews** | No automatic signatures or product credits in comments. |

If the environment appends trailers when you run `git commit`, fix the result before pushing (for example recreate the commit with `git commit-tree` so the message stays exactly what you intended).

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

The number is chosen in **Versioning**. The commit itself is only:

```
chore: version X.Y.Z
```

(no body required)

### What to avoid in commits

- Do not add co-authorship or agent trailers (see **No co-authorship** above).
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

- No co-authorship or agent attribution in the PR (see **No co-authorship**).

### Merge

- Prefer a **merge commit** (not squash-only) when the user asks to merge `develop` into `main`, so history on `develop` stays intact on `main`.
- After merge, `main` carries the merge commit; tags for releases are created on `main`.

## Versioning

DBunny is on **0.Y.Z**, the initial-development series of [Semantic Versioning](https://semver.org). In that series the spec lets anything change, so the rules below are the project's until **1.0.0**. The app version is the number in `src-tauri/tauri.conf.json` (kept in sync with `Cargo.toml` and `Cargo.lock`). The examples are the rule, not the current number.

A release moves one step, by the largest change since the previous tag:

| Change | Current | Next |
| --- | --- | --- |
| Bug fix | 0.5.0 | 0.5.1 |
| Another bug fix | 0.5.1 | 0.5.2 |
| Small improvement | 0.5.2 | 0.5.3 |
| New functionality | 0.5.3 | 0.6.0 |
| Later bug fix | 0.6.0 | 0.6.1 |
| New functionality | 0.6.1 | 0.7.0 |

- **Patch** — a bug fix (`fix`) or a small improvement (`improve`). No new capability.
- **Minor** — new functionality (`feat`). The patch goes back to 0.
- A release that mixes kinds takes the highest bump. Fixes and small improvements together stay on the patch. One new capability makes the next minor.
- `refactor`, `chore`, `docs`, `test` and `ci` do not bump the version by themselves.

**Incompatible changes before 1.0.0** (saved connections, the update path, or something a person already relies on stops working) are still a **minor** bump, not 1.0.0. That version's notes say what broke and what to do once. Do not repeat the note on later releases.

**1.0.0** is a deliberate release, not the next feature and not where an incompatible change lands on its own. Ship it when the user says the app should start keeping compatibility. From then on, follow SemVer as specified: patch for a compatible fix, minor for compatible new behavior, major for an incompatible change.

When preparing a release on `develop` (before merging to `main`), bump:

- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock` (the `dbunny` package only)
- `src-tauri/tauri.conf.json`

The tag is `vX.Y.Z` and must match that number.

## Releases

Typical flow used on this project:

1. Finish work on **`develop`**, then `chore: version X.Y.Z` using the next number from **Versioning**.
2. Open and merge PR **`develop`** → **`main`**.
3. On **`main`**, create and push tag **`vX.Y.Z`** (must match `tauri.conf.json`).
4. Tag push runs **`.github/workflows/build.yml`**: multi-platform Tauri build, draft GitHub Release, `latest.json` for the in-app updater. A follow-up job removes standalone `.sig` assets from the release.
5. **Edit the draft release** on GitHub: replace the auto-generated body with a full changelog (highlights, per-area notes, upgrading notes, download table). Include the **macOS quarantine** note for DMG installs:

   ```bash
   xattr -dr com.apple.quarantine "/Applications/DBunny.app"
   ```

6. **Publish** the release when assets are ready.

Write release notes and the README in English. Release text follows **No co-authorship** (no agent or tool credits). User-facing docs describe DBunny as a **multi-database** client; list supported engines accurately (see `README.md` and `web/src/lib/engines.ts`).

## CI

- **`test.yml`** — Runs on push to `develop` / `main` and on every PR: frontend unit tests, typecheck, Rust unit tests (the server tests stay ignored there).
- **`servers.yml`** — The server tests against every database version of the matrix (`src-tauri/tests/servers.sh list`), one job per version: weekly, on demand (Actions → Server tests, optionally one engine or version), and on pull requests to `main` that touch database or assistant code. A release PR (`develop` → `main`) needs it green.
- Fix CI failures before merging when possible.

## Local checks (before claiming “done”)

When you change code, run what applies:

```bash
npm test --workspace=web
npx tsc -b web
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

For database work, also run the server tests against real servers in Docker. `servers.sh` starts one version (only `dbunny-test-*` containers, ports on 127.0.0.1) and prints the variables the tests read; `matrix.sh` runs every version of the matrix one at a time and writes a summary to `src-tauri/target/matrix/summary.md`:

```bash
eval "$(src-tauri/tests/servers.sh up postgres 17)"
cargo test --manifest-path src-tauri/Cargo.toml --lib -- $(src-tauri/tests/servers.sh filters postgres) --ignored
src-tauri/tests/matrix.sh                 # all 16 versions
src-tauri/tests/matrix.sh mariadb 11.8    # one version
src-tauri/tests/servers.sh down all      # containers and their volumes
src-tauri/tests/servers.sh prune         # also the images of the matrix (about 13 GB)
```

A server test whose feature the version lacks returns early through `requires()` and prints a `dbunny-skip:` line; skipping is only for that, never for a failure.

## Languages

The interface speaks English and Portuguese (Brazil), and follows the system unless the person picks one in Settings. The code is in `web/src/i18n/`.

- **All text a person reads goes through `t()`** (`useT()` in components, `t` from `@/i18n` in handlers and `lib/`). Write the key in `web/src/i18n/locales/en.ts` and its translation in `pt-BR.ts` in the same commit; a missing key does not compile. Keys are `area.screen.role`, at most three levels deep, and words used everywhere live in `common`.
- **Never glue text together.** A sentence is one message with `{parameters}`; a noun is not inserted into another message; a count is a plural message (`{ one, other }`) with `{count}`. A text with a link or bold inside is read with `<Rich>`.
- **Counts, dates and lists use the formatters** (`useFormat()` or `fmt()`), not `toLocaleString`.
- **Errors the person sees:** an error DBunny itself makes in Rust is `AppError::coded("area.name", english_message).param(...)`, and its message goes in the catalog under `errors.area.name`. What a database server says passes through as it came. The interface shows any error with `errorText(error, fallback)`.
- **Words that stay English on purpose** (said to the agent, or the vocabulary of a database) carry `// i18n-ignore: why` on their line. The test in `web/test/i18n` finds text that skips `t()`, and its list of exceptions only shrinks.
- The pseudo language (the last item of the language picker in development) shows English made foreign and longer: use it to find text nobody extracted and layouts that don't stretch.
- README, release notes, PR text and commit messages stay in English.

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
