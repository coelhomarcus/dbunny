import { test } from "node:test";
import assert from "node:assert/strict";
import { splitStatements } from "../../src/lib/sqlStatements.ts";

const texts = (sql: string, dialect?: "postgres" | "sqlite") => splitStatements(sql, dialect).map((r) => sql.slice(r.from, r.to));

test("postgres splitting is unchanged", () => {
  assert.deepEqual(texts("select 1; select 2;"), ["select 1", "select 2"]);
  assert.deepEqual(texts("select $$a;b$$; select 2"), ["select $$a;b$$", "select 2"]);
  assert.deepEqual(texts("select E'a\\';b'; select 2"), ["select E'a\\';b'", "select 2"]);
  assert.deepEqual(texts("select 1 /* a /* b */; */; select 2"), ["select 1 /* a /* b */; */", "select 2"]);
  assert.deepEqual(texts("select $1; select 2"), ["select $1", "select 2"]);
  assert.deepEqual(texts("-- only a comment;\n"), []);
});

test("sqlite triggers are one statement", () => {
  const sql =
    "CREATE TABLE t(x); CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO t SELECT CASE WHEN new.x > 1 THEN 0 ELSE 1 END; SELECT ';'; END; SELECT 1;";
  const parts = texts(sql, "sqlite");
  assert.equal(parts.length, 3);
  assert.equal(parts[0], "CREATE TABLE t(x)");
  assert.ok(parts[1].startsWith("CREATE TRIGGER tr") && parts[1].endsWith("END"));
  assert.equal(parts[2], "SELECT 1");
});

test("a temp trigger and a trigger with a WHEN clause stay whole", () => {
  const sql = "CREATE TEMP TRIGGER a BEFORE UPDATE OF x ON t WHEN old.x <> new.x BEGIN UPDATE t SET y = 1; END; select 2";
  assert.deepEqual(texts(sql, "sqlite").length, 2);
  assert.deepEqual(texts("create temporary trigger a after delete on t begin select 1; end; select 2", "sqlite").length, 2);
});

test("begin and end outside a trigger split normally in sqlite", () => {
  assert.deepEqual(texts("BEGIN; INSERT INTO t VALUES (1); END; select 1", "sqlite"), ["BEGIN", "INSERT INTO t VALUES (1)", "END", "select 1"]);
  assert.deepEqual(texts("BEGIN IMMEDIATE; COMMIT;", "sqlite"), ["BEGIN IMMEDIATE", "COMMIT"]);
});

test("semicolons in sqlite strings, names and comments do not split", () => {
  assert.deepEqual(texts("select 'a;b'; select \"c;d\"", "sqlite"), ["select 'a;b'", 'select "c;d"']);
  assert.deepEqual(texts("select [a;b] from `c;d`; select 2", "sqlite"), ["select [a;b] from `c;d`", "select 2"]);
  assert.deepEqual(texts("select 1 -- a;b\n; select 2", "sqlite"), ["select 1 -- a;b", "select 2"]);
  assert.deepEqual(texts("select 1 /* a;b */; select 2", "sqlite"), ["select 1 /* a;b */", "select 2"]);
  assert.deepEqual(texts("select 'it''s;ok'; select 2", "sqlite"), ["select 'it''s;ok'", "select 2"]);
});

test("sqlite has no dollar quotes, no backslash escapes and no nested comments", () => {
  assert.deepEqual(texts("select $$a; select 2", "sqlite"), ["select $$a", "select 2"]);
  assert.deepEqual(texts("select E'a\\'; select 2", "sqlite"), ["select E'a\\'", "select 2"]);
  assert.deepEqual(texts("select 1 /* a /* b */; select 2", "sqlite"), ["select 1 /* a /* b */", "select 2"]);
});

test("a sqlite script may end without a semicolon, and empty statements are dropped", () => {
  assert.deepEqual(texts("select 1;; ; select 2", "sqlite"), ["select 1", "select 2"]);
  assert.deepEqual(texts("-- nothing;\n", "sqlite"), []);
});

test("a trigger ends at the END that follows a semicolon, like sqlite itself decides", () => {
  // begin and end are valid column names: they don't open or close the trigger
  const withBeginColumn =
    "CREATE TRIGGER t AFTER UPDATE ON ev BEGIN UPDATE ev SET begin = new.x WHERE id = new.id; END; DROP TABLE old_stuff; SELECT 1;";
  const parts = texts(withBeginColumn, "sqlite");
  assert.equal(parts.length, 3);
  assert.ok(parts[0].startsWith("CREATE TRIGGER t") && parts[0].endsWith("END"));
  assert.equal(parts[1], "DROP TABLE old_stuff");

  const withEndColumn = "CREATE TRIGGER t AFTER INSERT ON ev BEGIN UPDATE ev SET x = new.end; SELECT 1; END; SELECT 2";
  const second = texts(withEndColumn, "sqlite");
  assert.equal(second.length, 2);
  assert.ok(second[0].endsWith("SELECT 1; END"));
  assert.equal(second[1], "SELECT 2");
});

test("a trigger is recognised after EXPLAIN and with unicode names around END", () => {
  assert.deepEqual(texts("EXPLAIN CREATE TRIGGER a AFTER DELETE ON t BEGIN SELECT 1; END; SELECT 2", "sqlite").length, 2);
  // A name that only ends in "end" is not the END keyword
  const sql = "CREATE TRIGGER a AFTER DELETE ON t BEGIN UPDATE t SET é_end = 1; SELECT 1; END; SELECT 2";
  assert.equal(texts(sql, "sqlite").length, 2);
});
