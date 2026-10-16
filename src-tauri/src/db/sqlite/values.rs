//! SQLite values and types in the shapes the app already knows from PostgreSQL.

use std::fmt::Write;

use rusqlite::types::{ToSqlOutput, ValueRef};
use rusqlite::ToSql;
use serde_json::Value as JsonValue;

use crate::db::sql::filter::TextParam;
use crate::db::sql::json::int_to_json;

/// A value as the grid shows it. Integers beyond what JavaScript holds exactly become
/// text, infinite reals the text `Infinity`/`-Infinity`, and blobs `\x` and their hex
/// digits like a PostgreSQL bytea.
pub fn value_to_json(value: ValueRef<'_>) -> JsonValue {
    match value {
        ValueRef::Null => JsonValue::Null,
        ValueRef::Integer(n) => int_to_json(n),
        ValueRef::Real(f) if f.is_infinite() => JsonValue::String(if f > 0.0 { "Infinity" } else { "-Infinity" }.into()),
        // SQLite stores NaN as NULL, so every other real is a finite number
        ValueRef::Real(f) => serde_json::Number::from_f64(f).map(JsonValue::Number).unwrap_or(JsonValue::Null),
        ValueRef::Text(text) => JsonValue::String(String::from_utf8_lossy(text).into_owned()),
        ValueRef::Blob(bytes) => {
            let mut hex = String::with_capacity(2 + bytes.len() * 2);
            hex.push_str("\\x");
            for byte in bytes {
                let _ = write!(hex, "{byte:02x}");
            }
            JsonValue::String(hex)
        }
    }
}

/// Sent as text: the column's affinity turns numeric text into a number for INTEGER,
/// REAL and NUMERIC columns, the way PostgreSQL converts a text parameter.
impl ToSql for TextParam {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(ValueRef::Text(self.0.as_bytes())))
    }
}

/// The PostgreSQL type name the screens understand for a column declared as `declared`
/// (the cell editors pick by `dataType`). The order of the rules matters, and case doesn't.
/// `BOOLEAN` stays `numeric` on purpose: SQLite keeps 0/1, and the true/false editor
/// would write the text 'true'.
pub fn standard_type(declared: &str) -> &'static str {
    let t = declared.trim().to_ascii_uppercase();
    let has = |part: &str| t.contains(part);
    if has("JSON") {
        "json"
    } else if t == "DATE" {
        "date"
    } else if has("DATETIME") || has("TIMESTAMP") {
        "timestamp without time zone"
    } else if t == "TIME" {
        "time without time zone"
    } else if has("INT") {
        "bigint"
    } else if has("CHAR") || has("CLOB") || has("TEXT") {
        "text"
    } else if has("BLOB") || t.is_empty() {
        "bytea"
    } else if has("REAL") || has("FLOA") || has("DOUB") {
        "double precision"
    } else {
        "numeric"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn values_become_the_json_the_grid_shows() {
        assert_eq!(value_to_json(ValueRef::Null), JsonValue::Null);
        assert_eq!(value_to_json(ValueRef::Integer(42)), json!(42));
        assert_eq!(value_to_json(ValueRef::Integer(-9_007_199_254_740_991)), json!(-9_007_199_254_740_991_i64));
        assert_eq!(value_to_json(ValueRef::Integer(9_007_199_254_740_993)), json!("9007199254740993"));
        assert_eq!(value_to_json(ValueRef::Integer(i64::MIN)), json!("-9223372036854775808"));
        assert_eq!(value_to_json(ValueRef::Real(1.5)), json!(1.5));
        assert_eq!(value_to_json(ValueRef::Real(f64::INFINITY)), json!("Infinity"));
        assert_eq!(value_to_json(ValueRef::Real(f64::NEG_INFINITY)), json!("-Infinity"));
        assert_eq!(value_to_json(ValueRef::Text(b"it's")), json!("it's"));
        assert_eq!(value_to_json(ValueRef::Blob(&[0x00, 0xff, 0x1a])), json!("\\x00ff1a"));
        assert_eq!(value_to_json(ValueRef::Blob(&[])), json!("\\x"));
    }

    #[test]
    fn text_parameters_are_text_and_none_is_null() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let kind = |p: &dyn ToSql| conn.query_row("SELECT typeof(?1), ?1 IS NULL", [p], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))).unwrap();
        assert_eq!(kind(&TextParam("5".into())), ("text".to_string(), false));
        assert_eq!(kind(&Some(TextParam("".into()))), ("text".to_string(), false));
        assert_eq!(kind(&None::<TextParam>), ("null".to_string(), true));
    }

    #[test]
    fn each_rule_names_its_type() {
        let cases = [
            ("JSON", "json"),
            ("jsonb", "json"),
            ("DATE", "date"),
            ("DATETIME", "timestamp without time zone"),
            ("TIMESTAMP", "timestamp without time zone"),
            ("time", "time without time zone"),
            ("INTEGER", "bigint"),
            ("BIGINT", "bigint"),
            ("VARCHAR(80)", "text"),
            ("CLOB", "text"),
            ("text", "text"),
            ("BLOB", "bytea"),
            ("", "bytea"),
            ("REAL", "double precision"),
            ("FLOAT", "double precision"),
            ("DOUBLE PRECISION", "double precision"),
            ("BOOLEAN", "numeric"),
            ("NUMERIC(10,2)", "numeric"),
            ("DECIMAL", "numeric"),
        ];
        for (declared, expected) in cases {
            assert_eq!(standard_type(declared), expected, "{declared:?}");
        }
    }
}
