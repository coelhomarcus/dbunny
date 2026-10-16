//! SQLite values and types in the shapes the app already knows from PostgreSQL.

use std::fmt::Write;

use rusqlite::types::{ToSqlOutput, Value, ValueRef};
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

/// Sent as text, as it is. Only a column with INTEGER, REAL or NUMERIC affinity turns
/// numeric text into a number; one without a type or declared BLOB keeps the text, which
/// then neither equals nor sorts with the numbers stored there. So the table data binds
/// by the column's affinity instead (`Affinity`).
impl ToSql for TextParam {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(ValueRef::Text(self.0.as_bytes())))
    }
}

/// How a column converts what it's given, from its declared type by SQLite's rules (the
/// same order as `standard_type`). A view column that is an expression has no declared
/// type, so no affinity, which converts nothing, like BLOB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity {
    Integer,
    Text,
    /// Declared BLOB, or no type at all: values are kept as they come.
    Blob,
    Real,
    Numeric,
}

impl Affinity {
    pub fn of(declared: &str) -> Affinity {
        let t = declared.to_ascii_uppercase();
        let has = |part: &str| t.contains(part);
        if has("INT") {
            Affinity::Integer
        } else if has("CHAR") || has("CLOB") || has("TEXT") {
            Affinity::Text
        } else if has("BLOB") || t.trim().is_empty() {
            Affinity::Blob
        } else if has("REAL") || has("FLOA") || has("DOUB") {
            Affinity::Real
        } else {
            Affinity::Numeric
        }
    }

    /// A value from the grid as this column should get it. Numbers and booleans are
    /// numbers. Text is left to columns that convert it, except the infinities SQLite
    /// can't read; a column that converts nothing gets the value the text shows (see
    /// `shown`), so a blob, number or infinity the grid shows as text is written back as
    /// it was read (text that looks like a number becomes that number there).
    pub fn value(self, value: &JsonValue) -> Value {
        match value {
            JsonValue::Null => Value::Null,
            JsonValue::Bool(b) => Value::Integer(i64::from(*b)),
            JsonValue::Number(n) => match n.as_i64() {
                Some(i) => Value::Integer(i),
                None => Value::Real(n.as_f64().unwrap_or(f64::NAN)),
            },
            JsonValue::String(text) => self.text(text),
            other => Value::Text(other.to_string()),
        }
    }

    /// Text (typed, imported or a filter's operand) as this column should get it.
    pub fn text(self, text: &str) -> Value {
        match self {
            Affinity::Text => Value::Text(text.to_string()),
            Affinity::Integer | Affinity::Real | Affinity::Numeric => {
                infinity(text).map(Value::Real).unwrap_or_else(|| Value::Text(text.to_string()))
            }
            Affinity::Blob => shown(text).unwrap_or_else(|| Value::Text(text.to_string())),
        }
    }

    /// The values a key the grid loaded may stand for. A number is that number. Text in a
    /// column that converts nothing is that text, or a value the grid shows as text: a
    /// blob, an integer too big for JavaScript, an infinity.
    pub fn key(self, value: &JsonValue) -> Vec<Value> {
        match value {
            JsonValue::String(text) if self == Affinity::Blob => {
                let mut forms = vec![Value::Text(text.clone())];
                forms.extend(shown(text).filter(|v| match v {
                    Value::Integer(n) => int_to_json(*n).is_string(),
                    Value::Real(f) => f.is_infinite(),
                    _ => true,
                }));
                forms
            }
            other => vec![self.value(other)],
        }
    }

    /// The values a filter's `=`, `!=` or IN operand matches: in a column that converts
    /// nothing both the text and the value it reads as, so `1` finds the integer 1 and
    /// the text '1'.
    pub fn matched(self, text: &str) -> Vec<Value> {
        match self {
            Affinity::Blob => {
                let mut forms = vec![Value::Text(text.to_string())];
                forms.extend(shown(text));
                forms
            }
            _ => vec![self.text(text)],
        }
    }
}

/// The value `text` reads as when it is exactly how the grid shows one: `\x` and pairs
/// of hex digits for a blob, an integer, a real (only when the text names the number
/// exactly: no leading zeros or extra digits), or an infinity.
fn shown(text: &str) -> Option<Value> {
    if let Some(bytes) = blob(text) {
        return Some(Value::Blob(bytes));
    }
    if let Some(n) = text.parse::<i64>().ok().filter(|n| n.to_string() == text) {
        return Some(Value::Integer(n));
    }
    canonical_real(text).or_else(|| infinity(text)).map(Value::Real)
}

fn infinity(text: &str) -> Option<f64> {
    match text {
        "Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        _ => None,
    }
}

/// The bytes of `\x00ff`, in either case.
fn blob(text: &str) -> Option<Vec<u8>> {
    let hex = text.strip_prefix("\\x")?;
    if hex.len() % 2 != 0 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect()
}

/// A finite real written with a point or an exponent, without leading zeros, whose
/// digits are all the number's own (so storing it as a real loses nothing).
fn canonical_real(text: &str) -> Option<f64> {
    let body = text.strip_prefix('-').unwrap_or(text);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (mantissa, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let well_formed = digits(whole)
        && (whole == "0" || !whole.starts_with('0'))
        && fraction.is_none_or(digits)
        && exponent.is_none_or(|e| digits(e.strip_prefix(['+', '-']).unwrap_or(e)))
        && (fraction.is_some() || exponent.is_some());
    if !well_formed {
        return None;
    }
    let number = text.parse::<f64>().ok().filter(|n| n.is_finite())?;
    (significant_digits(&format!("{number:e}")) == significant_digits(text)).then_some(number)
}

/// The digits of a number's mantissa without leading or trailing zeros.
fn significant_digits(number: &str) -> String {
    let mantissa = number.split(['e', 'E']).next().unwrap_or("");
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    digits.trim_start_matches('0').trim_end_matches('0').to_string()
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
    fn affinity_follows_the_declared_type() {
        let cases = [
            ("INTEGER", Affinity::Integer),
            ("tinyint", Affinity::Integer),
            ("POINT", Affinity::Integer),
            ("VARCHAR(80)", Affinity::Text),
            ("CLOB", Affinity::Text),
            ("BLOB", Affinity::Blob),
            ("", Affinity::Blob),
            ("  ", Affinity::Blob),
            ("REAL", Affinity::Real),
            ("DOUBLE PRECISION", Affinity::Real),
            ("FLOATING POINT", Affinity::Integer),
            ("NUMERIC(10,2)", Affinity::Numeric),
            ("BOOLEAN", Affinity::Numeric),
            ("DATETIME", Affinity::Numeric),
            ("JSON", Affinity::Numeric),
        ];
        for (declared, expected) in cases {
            assert_eq!(Affinity::of(declared), expected, "{declared:?}");
        }
    }

    #[test]
    fn values_are_bound_by_affinity() {
        let text = |s: &str| Value::Text(s.into());
        assert_eq!(Affinity::Text.value(&json!(null)), Value::Null);
        assert_eq!(Affinity::Text.value(&json!(true)), Value::Integer(1));
        assert_eq!(Affinity::Blob.value(&json!(false)), Value::Integer(0));
        assert_eq!(Affinity::Text.value(&json!(7)), Value::Integer(7));
        assert_eq!(Affinity::Blob.value(&json!(2.5)), Value::Real(2.5));
        assert_eq!(Affinity::Blob.value(&json!(u64::MAX)), Value::Real(u64::MAX as f64));
        assert_eq!(Affinity::Numeric.value(&json!({"a": [1]})), text("{\"a\":[1]}"));

        // Columns that convert text get it as it is, but for the infinities
        for affinity in [Affinity::Integer, Affinity::Real, Affinity::Numeric] {
            assert_eq!(affinity.text("6"), text("6"));
            assert_eq!(affinity.text("\\x00"), text("\\x00"));
            assert_eq!(affinity.text("Infinity"), Value::Real(f64::INFINITY));
            assert_eq!(affinity.text("-Infinity"), Value::Real(f64::NEG_INFINITY));
        }
        assert_eq!(Affinity::Text.text("Infinity"), text("Infinity"));
        assert_eq!(Affinity::Text.text("6"), text("6"));

        // A column that converts nothing gets what the text shows, and only that
        let blob = |s: &str| Affinity::Blob.text(s);
        assert_eq!(blob("\\x00fF"), Value::Blob(vec![0x00, 0xff]));
        assert_eq!(blob("\\x"), Value::Blob(vec![]));
        assert_eq!(blob("6"), Value::Integer(6));
        assert_eq!(blob("-12"), Value::Integer(-12));
        assert_eq!(blob("9223372036854775807"), Value::Integer(i64::MAX));
        assert_eq!(blob("2.5"), Value::Real(2.5));
        assert_eq!(blob("-0.125"), Value::Real(-0.125));
        assert_eq!(blob("1e5"), Value::Real(100_000.0));
        assert_eq!(blob("2.50"), Value::Real(2.5));
        assert_eq!(blob("0.30000000000000004"), Value::Real(0.30000000000000004));
        assert_eq!(blob("Infinity"), Value::Real(f64::INFINITY));
        for kept in ["007", "-0", "+5", "9223372036854775808", "1.", ".5", "01.5", "1e999", "0.1000000000000000000001", "inf", "NaN", "1_000", " 6", "\\x0", "\\xzz", "x00", "abc", ""] {
            assert_eq!(blob(kept), text(kept), "{kept:?}");
        }
    }

    #[test]
    fn keys_and_filters_match_every_form_the_grid_shows() {
        let text = |s: &str| Value::Text(s.into());
        // A number is that number; text in a column that converts is one value
        assert_eq!(Affinity::Blob.key(&json!(1)), vec![Value::Integer(1)]);
        assert_eq!(Affinity::Integer.key(&json!("9007199254740993")), vec![text("9007199254740993")]);
        assert_eq!(Affinity::Real.key(&json!("Infinity")), vec![Value::Real(f64::INFINITY)]);
        // Text in a column that converts nothing: the text, or what the grid shows as text
        assert_eq!(Affinity::Blob.key(&json!("1")), vec![text("1")]);
        assert_eq!(Affinity::Blob.key(&json!("2.5")), vec![text("2.5")]);
        assert_eq!(Affinity::Blob.key(&json!("\\x00ff")), vec![text("\\x00ff"), Value::Blob(vec![0, 255])]);
        assert_eq!(Affinity::Blob.key(&json!("9007199254740993")), vec![text("9007199254740993"), Value::Integer(9_007_199_254_740_993)]);
        assert_eq!(Affinity::Blob.key(&json!("-Infinity")), vec![text("-Infinity"), Value::Real(f64::NEG_INFINITY)]);

        assert_eq!(Affinity::Blob.matched("1"), vec![text("1"), Value::Integer(1)]);
        assert_eq!(Affinity::Blob.matched("2.5"), vec![text("2.5"), Value::Real(2.5)]);
        assert_eq!(Affinity::Blob.matched("abc"), vec![text("abc")]);
        assert_eq!(Affinity::Integer.matched("1"), vec![text("1")]);
        assert_eq!(Affinity::Text.matched("1"), vec![text("1")]);
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
