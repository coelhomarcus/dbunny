# The SQL editor's autocomplete

What the query editor suggests as you type, where each suggestion comes from, and how it stays in the SQL of the connected database. It describes the code as of version 0.14.0.

- [What it offers](#what-it-offers)
- [Where the cursor is](#where-the-cursor-is)
- [One dialect per database](#one-dialect-per-database)
- [Functions](#functions)
- [Snippets](#snippets)
- [What depends on the server's version](#what-depends-on-the-servers-version)
- [What the server sends](#what-the-server-sends)
- [How a suggestion is built](#how-a-suggestion-is-built)
- [How the code is organised](#how-the-code-is-organised)
- [Changing it](#changing-it)
- [Testing](#testing)

## What it offers

Every suggestion has a kind. The kind picks its icon and its color, and its name is written next to it, so the icon isn't the only clue.

| Kind | Icon | Color | What it is |
|---|---|---|---|
| `schema` | Database | neutral | A schema in PostgreSQL, or a database in MySQL and MariaDB |
| `table` | Table | blue | A table (partitioned and foreign tables included) |
| `view` | Eye | purple | A view |
| `materializedView` | Layers | purple | A materialized view (PostgreSQL) |
| `column` | Columns | neutral | A column |
| `columnPrimaryKey` | Key | amber | A column of the primary key |
| `columnForeignKey` | Link | amber | A column with a foreign key, and the ready-made `JOIN … ON` conditions |
| `function` | Function | green | A function the user created |
| `procedure` | Terminal | green | A procedure the user created (never in SQLite, which has none) |
| `builtinFunction` | Sigma | green | A function of the database itself |
| `keyword` | Hash | gray | A keyword or a phrase such as `LEFT JOIN` |
| `dataType` | Shapes | gray | A data type |
| `snippet` | Braces | light blue | A statement template |
| `local` | Variable | neutral | A name the query makes up: an alias, a CTE, a word already in the editor |

The icons are lucide icons, drawn once as SVG (`renderToStaticMarkup`) and put before each label by CodeMirror's `addToOptions`. They are `aria-hidden`; the text next to them carries the meaning.

**The detail** (to the right of the label):

- column: its type, then `PK` or `FK → table`, and the table it belongs to when the query reads more than one;
- table and view: the kind; a relation outside the current schema shows as `schema.name`;
- function of the user: its arguments and return type;
- catalog function, keyword, data type: the kind;
- snippet: what it does (`Add a row, or change the one with the same key`).

**The side panel** (`info`):

- catalog function: its signature and a description, in the interface's language;
- column: its type, whether it accepts null, and what its foreign key references;
- snippet: the statement it writes.

**The order.** Each kind has a `boost`, so the list reads from what the position asks for down to plain words:

| Boost | What |
|---|---|
| 60 | `JOIN … ON` conditions from foreign keys |
| 55 | After `JOIN`, the tables tied by a foreign key to the ones already in the query |
| 52 | CTEs where a table goes |
| 50 | What the position asks for: tables after `FROM`, columns after `SELECT`… |
| 45 | Aliases of the query |
| 40 | Other schemas |
| 20 / 18 / 10 | Catalog functions / the user's functions / names only the server knows |
| 0 | Snippets |
| −8 / −10 / −12 | Phrases / keywords / data types |
| −20 | Other words of the document |

## Where the cursor is

The editor reads the statement under the cursor and decides what fits there. Nothing is suggested inside a string or a comment.

| Position | Example | Suggested |
|---|---|---|
| Where a table goes | `FROM ▌`, `JOIN ▌`, `UPDATE ▌`, `INSERT INTO ▌`, `DELETE FROM ▌`, `TRUNCATE TABLE ▌`, `FROM a, ▌` | CTEs, tables and views, other schemas; after `JOIN`, related tables first |
| Join condition | `JOIN customers c ON ▌` | `c.id = o.customer_id` for each foreign key between the joined table and the ones before it, then the columns of all of them |
| After `name.` | `o.▌`, `sales.▌`, `sales.orders.▌` | The columns of that alias, table or CTE; for a schema, its tables, views and functions; `schema.table.` is always that schema's table |
| Expression | after `SELECT`, `WHERE`, `AND`, `OR`, `ON`, `SET`, `GROUP BY`, `ORDER BY`, `HAVING`, `ON DUPLICATE KEY UPDATE`, `DO UPDATE SET`, a comma or an operator; `RETURNING` where the server has it | Columns of the relations the query reads, their aliases, functions, `case`, keywords |
| Insert columns | `INSERT INTO orders (id, ▌` | Only the columns of `orders` that aren't in the list yet |
| Start of a statement | an empty editor, after `;` | Snippets and keywords |
| After a whole expression or relation | `FROM orders o ▌`, `SELECT a ▌` | Keywords and phrases (`WHERE`, `LEFT JOIN`), `join` |
| Nothing to say | inside `'…'`, `-- …`, `/* … */`, `$$ … $$`, or naming a new alias (`AS ▌`) | Nothing |

With nothing typed yet, the list only opens by itself where something specific is expected: a table, a column (once the query reads a relation), a join condition, a name after a dot, an insert column. Elsewhere it waits for a letter, or for Ctrl-Space.

**What the query reads.** The relations of the statement come with their schema and alias (`FROM sales.orders o`, `JOIN customers AS c`, comma lists, `UPDATE`, `INSERT INTO t (…)`, `DELETE FROM`), and with how the query writes them: a join condition or an alias is written back exactly as typed, so an unquoted `Orders` in PostgreSQL never comes back as `"Orders"`. A MySQL or MariaDB function that shares its name with a clause (`TRUNCATE(x, 2)`, `VALUES(col)`, `INSERT(…)`) doesn't end the expression it's in. CTEs come with the columns they declare (`WITH totals(customer, amount) AS (…)`, `RECURSIVE`, `MATERIALIZED`). Each relation remembers the parentheses it sits in:

- a relation inside a CTE body or a `WHERE … IN (SELECT …)` doesn't reach the cursor outside it;
- a correlated subquery sees the relations of the query around it.

**What the statement is.** The statement under the cursor is cut by the same splitter that decides what Run executes (`lib/sqlStatements.ts`), not by the nearest `;`, so a `;` inside a string or a procedure body doesn't cut it.

## One dialect per database

Each database has its own dialect module, and the editor only uses the one of the connected database. What one database writes never appears in another.

| | PostgreSQL | MySQL | MariaDB | SQLite |
|---|---|---|---|---|
| Names quoted with | `"…"` | `` `…` `` | `` `…` `` | `"…"` (also reads `` `…` `` and `[…]`) |
| A name is quoted when | it's one of PostgreSQL's reserved words, has another character, or has an upper-case letter | it's one of MySQL's words or has another character | as MySQL | it's one of SQLite's words or has another character |
| Strings | `'…'`, `E'…'` with `\` escapes, `$tag$…$tag$` | `'…'` and `"…"`, with `\` escapes | as MySQL | `'…'` |
| Line comments | `--` | `--` and `#` | `--` and `#` | `--` |
| Block comments | `/* */`, nested | `/* */` | `/* */` | `/* */` |
| Editor language (highlighting) | `PostgreSQL` | `MySQL` | `MariaSQL` | `SQLite` |
| Catalog functions | 121 | 116 | 116 | 74 |
| Snippets | 10 | 9 | 9 | 9 |

**Quoting.** PostgreSQL quotes only the words it reserves (Appendix C of its manual): `id`, `name`, `type` and `index` stay as they are, `user` and `select` are quoted. The other dialects quote every word `@codemirror/lang-sql` lists for them. A name is only ever quoted when it has to be.

**Keywords and types** come from `@codemirror/lang-sql`'s list for each dialect, which follows the official keyword pages. The dialect adds the phrases of several words:

- **every dialect:** `INNER JOIN`, `LEFT JOIN`, `LEFT OUTER JOIN`, `RIGHT JOIN`, `CROSS JOIN`, `IS NULL`, `IS NOT NULL`, `NOT NULL`, `ORDER BY`, `GROUP BY`, `PARTITION BY`, `UNION ALL`, `PRIMARY KEY`, `FOREIGN KEY`, `INSERT INTO`, `DELETE FROM`, `IF EXISTS`, `IF NOT EXISTS`;
- **PostgreSQL:** `FULL JOIN`, `FULL OUTER JOIN`, `LEFT JOIN LATERAL`, `CROSS JOIN LATERAL`, `ON CONFLICT`, `DO NOTHING`, `DO UPDATE SET`, `NULLS FIRST`, `NULLS LAST`, `NULLS NOT DISTINCT`, `IS [NOT] DISTINCT FROM`, `DISTINCT ON`, `FOR UPDATE`, `SKIP LOCKED`, `WITH RECURSIVE`, `CREATE OR REPLACE`, `MERGE INTO`;
- **MySQL and MariaDB:** `ON DUPLICATE KEY UPDATE`, `INSERT IGNORE`, `STRAIGHT_JOIN`, `FOR UPDATE`, `SKIP LOCKED`, `WITH RECURSIVE`, `CREATE OR REPLACE`; MySQL also `LEFT JOIN LATERAL`;
- **SQLite:** `FULL JOIN`, `ON CONFLICT`, `DO NOTHING`, `DO UPDATE SET`, `NULLS FIRST`, `NULLS LAST`, `IS [NOT] DISTINCT FROM`, `INSERT OR REPLACE`, `INSERT OR IGNORE`, `WITH RECURSIVE`, `WITHOUT ROWID`.

**What never crosses over**, checked by the tests:

- MySQL and MariaDB never get `ON CONFLICT`, `NULLS FIRST`, `DO NOTHING`, `FULL JOIN`, `ILIKE`, `INSERT OR REPLACE` or `date_trunc`;
- PostgreSQL and SQLite never get `ON DUPLICATE KEY UPDATE`; PostgreSQL never gets `DATE_FORMAT` or `GROUP_CONCAT`;
- functions that only one database has stay in its catalog: `date_trunc`, `generate_series`, `jsonb_set`, `to_char`, `age`, `gen_random_uuid`, `array_agg`, `pg_typeof` (PostgreSQL); `UUID_TO_BIN`, `BIN_TO_UUID`, `REGEXP_LIKE`, `JSON_PRETTY`, `STATEMENT_DIGEST` (MySQL); `NVL`, `JSON_DETAILED`, `JSON_VALUE`, `JSON_QUERY` (MariaDB); `DATE_FORMAT`, `STR_TO_DATE`, `TIMESTAMPDIFF`, `DATEDIFF`, `CURDATE`, `LAST_INSERT_ID` (MySQL and MariaDB); `strftime`, `julianday`, `unixepoch`, `last_insert_rowid`, `sqlite_version`, `total`, `iif`, `json_group_array` (SQLite);
- SQLite never gets a procedure.

`MD5` is in the MariaDB catalog and not in MySQL's: MySQL 26.10 no longer has it, and every catalog function has to run on every version of the matrix.

## Functions

Functions come from three places, merged into one list where each name appears once:

1. **The catalog** (`catalog/<engine>.json`): the functions of everyday use of each database, in eight categories (text, numbers, date and time, JSON, aggregate, window, conditional, system). Each has a signature, a description in English and Portuguese, and an example that runs. These come first.
2. **The user's functions and procedures**, read from the database with their arguments and return type. A function outside the current schema shows as `schema.name`; overloads come once.
3. **Names only the server knows**: everything else PostgreSQL has in `pg_catalog` and SQLite in `pragma_function_list`, without a description. MySQL and MariaDB don't list their built-in functions, so they rely on the catalog.

Names follow each manual: lower case in PostgreSQL and SQLite, upper case in MySQL and MariaDB. A description's key is `sqlFunctions.<engine>.<name>` with the name in camelCase (`date_trunc` → `dateTrunc`, `CHAR_LENGTH` → `charLength`).

Some functions exist only from a version on. Their catalog entry says which feature they need (`"requires": "jsonTable"`), and the editor leaves them out on an older server: `JSON_TABLE` on MySQL and MariaDB, `JSON_VALUE`, `JSON_QUERY` and `JSON_EXISTS` on PostgreSQL 17, `JSON_ARRAY` on PostgreSQL 16.

## Snippets

A snippet writes a whole statement with fields to fill in; Tab moves to the next field. They are offered when a word is typed that matches their label: at the start of a statement, `case` inside an expression, `join` after a relation.

| Label | PostgreSQL | MySQL | MariaDB | SQLite |
|---|---|---|---|---|
| `select` | `SELECT … FROM … WHERE …` | same | same | same |
| `insert` | `INSERT INTO … (…) VALUES (…)` | same | same | same |
| `update` | `UPDATE … SET … = … WHERE …` | same | same | same |
| `delete` | `DELETE FROM … WHERE …` | same | same | same |
| `with` | `WITH name AS (…) SELECT … FROM name` | same | same | same |
| `case` | `CASE WHEN … THEN … ELSE … END` | same | same | same |
| `join` | `JOIN … ON …` | same | same | same |
| `upsert` | `ON CONFLICT (key) DO UPDATE SET col = EXCLUDED.col` | `VALUES (…) AS new ON DUPLICATE KEY UPDATE col = new.col` | `ON DUPLICATE KEY UPDATE col = VALUES(col)` | `ON CONFLICT (key) DO UPDATE SET col = excluded.col` |
| `create table` | `id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY` | `id BIGINT AUTO_INCREMENT PRIMARY KEY` | as MySQL | `id INTEGER PRIMARY KEY` |
| `merge` | `MERGE INTO … USING … ON … WHEN MATCHED … WHEN NOT MATCHED …` (15 and later) | — | — | — |

They live in `catalog/<engine>.snippets.json`, read both by the interface and by the server tests. What a snippet does is said in the interface's language (`sqlSnippets.<id>`); the label stays the SQL word.

## What depends on the server's version

When it connects, the backend works out the SQL the server speaks (`sql_features` in `src-tauri/src/db/version.rs`) and sends it with the capabilities (`capabilities.sqlFeatures`). Phrases, functions and snippets that need a feature the server lacks aren't offered. The interface never compares versions itself.

| Feature | PostgreSQL | MySQL | MariaDB | SQLite | What it gates |
|---|---|---|---|---|---|
| `windowFunctions` | yes | yes | yes | yes | |
| `recursiveCte` | yes | yes | yes | yes | `WITH RECURSIVE` |
| `returning` | yes | — | yes | yes | columns after `RETURNING` |
| `returningUpdate` | yes | — | — | yes | |
| `onConflict` | yes | — | — | yes | `ON CONFLICT`, `DO NOTHING`, `DO UPDATE SET`, `upsert` |
| `onDuplicateKeyUpdate` | — | yes | yes | — | `ON DUPLICATE KEY UPDATE`, `upsert` |
| `nullsOrdering` | yes | — | — | yes | `NULLS FIRST`, `NULLS LAST` |
| `ilike` | yes | — | — | — | |
| `lateral` | yes | 8.0.14 | — | — | `LEFT JOIN LATERAL`, `CROSS JOIN LATERAL` |
| `filterClause` | yes | — | — | yes | |
| `merge` | 15 | — | — | — | `MERGE INTO`, `merge` |
| `nullsNotDistinct` | 15 | — | — | — | `NULLS NOT DISTINCT` |
| `sqlJsonConstructors` | 16 | — | — | — | `JSON_ARRAY` |
| `jsonTable` | 17 | yes | yes | — | `JSON_TABLE`; PostgreSQL's `JSON_VALUE`, `JSON_QUERY`, `JSON_EXISTS` |

The assistant's notes about what a server lacks are derived from the same list.

## What the server sends

`get_completion_schema` returns a `CompletionSchema` (`src-tauri/src/db/model.rs`). The query pane asks for it when it opens and again after a statement that changes the structure (`CREATE`, `ALTER`, `DROP`…). Until it arrives, the dialect's keywords, phrases, catalog functions and snippets are already offered.

| Field | PostgreSQL | MySQL and MariaDB | SQLite |
|---|---|---|---|
| `currentSchema` | `current_schema()` | the editor's database | `main` |
| `relations[].type` | `table`, `view` or `materializedView` from `relkind` | `table` or `view` | `table` or `view` |
| `columns[].primaryKey` | the primary key constraint | `COLUMN_KEY = 'PRI'` | `pk` of `table_xinfo` |
| `columns[].nullable` | `attnotnull` | `IS_NULLABLE` | `notnull` and `pk` |
| `columns[].references` | the foreign key constraint, column by column | `KEY_COLUMN_USAGE.REFERENCED_*` | `pragma foreign_key_list` |
| `functions[]` (with `kind`) | the user's schemas, `prokind` | `ROUTINES`, `ROUTINE_TYPE` | none |
| `builtinFunctions` | `pg_catalog`, without names starting with `_` and type support functions | empty | `pragma_function_list` |

A foreign key of several columns marks each column with its own target, so a join condition comes out for each pair.

## How a suggestion is built

1. CodeMirror calls the editor's source on each keystroke (`completion/index.ts`).
2. `analyze` (`completion/context.ts`) cuts the statement under the cursor, splits it into tokens the way the dialect reads it (strings, quoted names, comments), and finds the relations, CTEs and aliases in reach of the cursor. Then it walks back from the cursor to the nearest clause at the same depth of parentheses, which gives the position.
3. Inside a string or a comment it stops. With nothing typed and a position that waits for a letter, it stops too.
4. `optionsAt` (`completion/sources.ts`) gathers what the position asks for: relations, columns, join conditions, functions, snippets, keywords. Each option has its kind, detail, panel text, boost, and the name as the dialect writes it (`apply`) when that differs from the label.
5. A name typed after an opening quote (`"Ord`) is matched without the quote and completed without adding another. Only single names are offered there: `schema.table` and join conditions would end up inside one pair of quotes.
6. Once a word is typed, the other words of the document that nothing else suggests are added as `local`, last.
7. CodeMirror filters by what was typed, sorts by match and boost, and draws each item with its icon.

The schema is indexed once per catalog (by name, and its schemas), and the function and keyword lists are built once per dialect and set of features. A test checks that a catalog of 2,000 tables of 20 columns and 3,000 built-in names is answered in under 50 ms.

## How the code is organised

Everything is in the interface, under `web/src/lib/sql/`.

| File | What it does |
|---|---|
| `dialects/types.ts` | `SqlDialect`, the contract of a dialect: language, quoting, comments, keywords, types, phrases, functions, snippets |
| `dialects/postgres.ts`, `mysql.ts`, `mariadb.ts`, `sqlite.ts` | One module per database; MariaDB shares MySQL's phrases |
| `dialects/shared.ts` | The phrases every dialect has, the word lists of `lang-sql`, the quoting rule |
| `dialects/catalog/<engine>.json` | The catalog of functions |
| `dialects/catalog/<engine>.snippets.json` | The snippets |
| `dialects/index.ts` | `dialectOf(engine)`, `languageFor(engine)`, `available(items, features)`, `functionKey(name)` |
| `completion/context.ts` | `analyze`: the tokenizer, the statement's relations and CTEs, the position |
| `completion/sources.ts` | The suggestions of each kind, and `optionsAt` |
| `completion/icons.ts` | The kinds, their icons and their names |
| `completion/index.ts` | `sqlCompletion(schema, engine, features)`, the CodeMirror source, and `completionIcon` |
| `codemirror/theme.ts` | The icon colors and the side panel |

`components/SqlEditor.tsx` passes the connected engine, `capabilities.sqlFeatures` and the `CompletionSchema`; `components/SqlViewer.tsx` highlights with `languageFor(engine)`. Descriptions and names are in `web/src/i18n/locales/en.ts` and `pt-BR.ts` under `sqlFunctions`, `sqlSnippets` and `ui.sqlCompletion`.

On the backend, `sql_features` is in `src-tauri/src/db/version.rs` and the `CompletionSchema` of each engine in its `catalog.rs`.

## Changing it

**A function.** Add it to `catalog/<engine>.json` with `name`, `signature`, `category`, an `example` that runs on every version of the matrix, and `requires` if it needs a newer server. Add its description under `sqlFunctions.<engine>.<key>` in `en.ts` and `pt-BR.ts`. If only one database has it, add it to the exclusive list of `web/test/lib/sql/dialects.test.ts`.

**A snippet.** Add it to `catalog/<engine>.snippets.json` with `id`, `label`, `template` (fields as `${name}`; the same name twice is one field), `example` (written against `dbunny_snippet_t (id, name)`) and `requires` if needed. Say what it does under `sqlSnippets.<id>`, and where it fits in `SNIPPETS_AT` of `completion/sources.ts`.

**A phrase.** Add it to the dialect's module, with `requires` when it depends on the version.

**A feature of a version.** Add it to `sql_features` in `version.rs` with its unit tests, and to `SqlFeature` in `web/src/types/capabilities.ts`.

**A database.** Write its module against `SqlDialect`, its two JSON files and its descriptions, add it to `DIALECTS` in `dialects/index.ts`, and add its catalog tests to `src-tauri/src/db/sql/catalog_tests.rs`. The tests that go through every engine will fail until it's complete.

## Testing

- **Interface** (`npm test --workspace=web`):
  - `web/test/lib/sql/dialects.test.ts`: every engine has a dialect, quoting, phrases and functions that never cross over, complete catalogs, descriptions in both languages;
  - `web/test/lib/sql/context.test.ts`: every position, strings and comments of each dialect, aliases, CTEs, scopes, schemas with the same table, the statement between others;
  - `web/test/lib/sql/completion.test.ts`: keys, join conditions, related tables, the merge of functions, no leaks, `requires`, quoted names, icons, snippets, the order, a large catalog under 50 ms.
- **Servers** (`src-tauri/src/db/sql/catalog_tests.rs`): every catalog example and every snippet runs on the server it belongs to, on every version of the matrix; a function or snippet whose feature the version lacks isn't run. Snippets run in a transaction that is rolled back, MySQL and MariaDB in a throwaway database (a `CREATE TABLE` commits by itself there). SQLite runs both on the bundled library in the unit tests.
- The `CompletionSchema` of each engine has its own server tests: keys, nullability, foreign keys of one and of several columns, views and materialized views, procedures, built-in names.

How to run the server tests against the 16 versions is in [DATABASES.md](DATABASES.md#testing).
