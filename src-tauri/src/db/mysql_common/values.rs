//! MySQL values and types in the shapes the grid already knows from PostgreSQL, and the
//! parameters that write them back.

use std::collections::HashMap;
use std::fmt::Write;

use mysql_async::consts::{ColumnFlags, ColumnType};
use mysql_async::{Column, Value};
use serde_json::Value as JsonValue;

use crate::db::sql::bind::{OperandBinder, ValueBinder};
use crate::db::sql::json::int_to_json;

/// The binary character set: what a column holds is bytes, not text. Every numeric and
/// temporal column reports it too, so it only means "binary" for string and blob types.
const BINARY_CHARSET: u16 = 63;

/// The most digits a time or datetime has after the point.
const MAX_FRACTION_DIGITS: u8 = 6;

/// A value as the grid shows it (spec 6.2.1). Takes both protocols: in the text one nearly
/// everything comes as `Bytes` and the type of the column decides, in the binary one as
/// `Int`, `UInt`, `Float`, `Double`, `Date` and `Time`.
///
/// Integers, `BIT` and `YEAR` are numbers while JavaScript holds them exactly and text
/// beyond. Decimals, dates and times stay text, with zero dates and `-838:59:59` as the
/// server writes them. `JSON` is a JSON value, binary columns `\x` and their hex digits
/// like a PostgreSQL bytea.
pub fn value_to_json(value: &Value, column: &Column) -> JsonValue {
    match value {
        Value::NULL => JsonValue::Null,
        Value::Int(n) => int_to_json(*n),
        Value::UInt(n) => unsigned_to_json(*n),
        // The shortest text of the f32, as the text protocol writes it, not the f64 it widens to
        Value::Float(f) => float_to_json(f.to_string().parse::<f64>().unwrap_or(f64::NAN)),
        Value::Double(f) => float_to_json(*f),
        Value::Date(year, month, day, hour, minute, second, micro) => {
            let mut text = format!("{year:04}-{month:02}-{day:02}");
            if !matches!(column.column_type(), ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE) {
                let _ = write!(text, " {hour:02}:{minute:02}:{second:02}");
                fraction(&mut text, *micro, column);
            }
            JsonValue::String(text)
        }
        Value::Time(negative, days, hours, minutes, seconds, micro) => {
            let sign = if *negative { "-" } else { "" };
            let mut text = format!("{sign}{:02}:{minutes:02}:{seconds:02}", days * 24 + u32::from(*hours));
            fraction(&mut text, *micro, column);
            JsonValue::String(text)
        }
        Value::Bytes(bytes) => bytes_to_json(bytes, column),
    }
}

/// What the text protocol sends for everything, and the binary one for strings, decimals,
/// `BIT` and `JSON`.
fn bytes_to_json(bytes: &[u8], column: &Column) -> JsonValue {
    use ColumnType::*;
    let text = String::from_utf8_lossy(bytes);
    match column.column_type() {
        MYSQL_TYPE_TINY | MYSQL_TYPE_SHORT | MYSQL_TYPE_INT24 | MYSQL_TYPE_LONG | MYSQL_TYPE_LONGLONG | MYSQL_TYPE_YEAR => {
            if let Ok(n) = text.parse::<i64>() {
                int_to_json(n)
            } else if let Ok(n) = text.parse::<u64>() {
                unsigned_to_json(n)
            } else {
                JsonValue::String(text.into_owned())
            }
        }
        MYSQL_TYPE_FLOAT | MYSQL_TYPE_DOUBLE => match text.parse::<f64>() {
            Ok(f) => float_to_json(f),
            Err(_) => JsonValue::String(text.into_owned()),
        },
        // Big-endian, as many bytes as the column has bits (8 at most)
        MYSQL_TYPE_BIT if bytes.len() <= 8 => unsigned_to_json(bytes.iter().fold(0u64, |n, byte| (n << 8) | u64::from(*byte))),
        // JSON reports the binary character set, so it goes before the check for binary
        MYSQL_TYPE_JSON => serde_json::from_slice(bytes).unwrap_or_else(|_| JsonValue::String(text.into_owned())),
        MYSQL_TYPE_VARCHAR | MYSQL_TYPE_VAR_STRING | MYSQL_TYPE_STRING | MYSQL_TYPE_TINY_BLOB | MYSQL_TYPE_MEDIUM_BLOB | MYSQL_TYPE_LONG_BLOB
        | MYSQL_TYPE_BLOB
            if !is_binary(column) =>
        {
            JsonValue::String(text.into_owned())
        }
        // Binary strings and blobs; geometry and vectors have no text form of their own
        MYSQL_TYPE_VARCHAR | MYSQL_TYPE_VAR_STRING | MYSQL_TYPE_STRING | MYSQL_TYPE_TINY_BLOB | MYSQL_TYPE_MEDIUM_BLOB | MYSQL_TYPE_LONG_BLOB
        | MYSQL_TYPE_BLOB | MYSQL_TYPE_GEOMETRY | MYSQL_TYPE_VECTOR | MYSQL_TYPE_BIT => hex(bytes),
        _ => JsonValue::String(text.into_owned()),
    }
}

/// An unsigned integer: a number up to what JavaScript holds exactly.
fn unsigned_to_json(n: u64) -> JsonValue {
    match i64::try_from(n) {
        Ok(n) => int_to_json(n),
        Err(_) => JsonValue::String(n.to_string()),
    }
}

/// A float as a number. MySQL has no NaN or infinity, so anything else is NULL.
fn float_to_json(f: f64) -> JsonValue {
    serde_json::Number::from_f64(f).map(JsonValue::Number).unwrap_or(JsonValue::Null)
}

/// `\x` and the lowercase hex digits of `bytes`.
fn hex(bytes: &[u8]) -> JsonValue {
    let mut text = String::with_capacity(2 + bytes.len() * 2);
    text.push_str("\\x");
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    JsonValue::String(text)
}

/// Whether a string or blob column holds bytes (`BINARY`, `VARBINARY`, `BLOB`), not text.
fn is_binary(column: &Column) -> bool {
    column.character_set() == BINARY_CHARSET
}

/// The fraction of a time or datetime as the text protocol writes it: the digits the column
/// was declared with (`TIME(3)` always has three), or six when only the value has one.
fn fraction(text: &mut String, micro: u32, column: &Column) {
    let digits = match column.decimals() {
        d @ 1..=MAX_FRACTION_DIGITS => d,
        _ if micro != 0 => MAX_FRACTION_DIGITS,
        _ => return,
    };
    let shown = micro / 10u32.pow(u32::from(MAX_FRACTION_DIGITS - digits));
    let _ = write!(text, ".{shown:0width$}", width = usize::from(digits));
}

/// The name of a result column's type, as `QueryColumn.data_type` carries it ("int",
/// "varchar", "blob", "json"...). Unsigned doesn't change it.
pub fn type_name(column: &Column) -> String {
    use ColumnType::*;
    let flags = column.flags();
    let pick = |binary: &'static str, text: &'static str| if is_binary(column) { binary } else { text };
    match column.column_type() {
        MYSQL_TYPE_TINY => "tinyint",
        MYSQL_TYPE_SHORT => "smallint",
        MYSQL_TYPE_INT24 => "mediumint",
        MYSQL_TYPE_LONG => "int",
        MYSQL_TYPE_LONGLONG => "bigint",
        MYSQL_TYPE_DECIMAL | MYSQL_TYPE_NEWDECIMAL => "decimal",
        MYSQL_TYPE_FLOAT => "float",
        MYSQL_TYPE_DOUBLE => "double",
        MYSQL_TYPE_DATE | MYSQL_TYPE_NEWDATE => "date",
        MYSQL_TYPE_DATETIME | MYSQL_TYPE_DATETIME2 => "datetime",
        MYSQL_TYPE_TIMESTAMP | MYSQL_TYPE_TIMESTAMP2 => "timestamp",
        MYSQL_TYPE_TIME | MYSQL_TYPE_TIME2 => "time",
        MYSQL_TYPE_YEAR => "year",
        MYSQL_TYPE_BIT => "bit",
        MYSQL_TYPE_JSON => "json",
        MYSQL_TYPE_ENUM => "enum",
        MYSQL_TYPE_SET => "set",
        // The server describes an ENUM or a SET as a string and says which one in the flags
        MYSQL_TYPE_STRING if flags.contains(ColumnFlags::ENUM_FLAG) => "enum",
        MYSQL_TYPE_STRING if flags.contains(ColumnFlags::SET_FLAG) => "set",
        MYSQL_TYPE_STRING => pick("binary", "char"),
        MYSQL_TYPE_VARCHAR | MYSQL_TYPE_VAR_STRING => pick("varbinary", "varchar"),
        MYSQL_TYPE_TINY_BLOB => pick("tinyblob", "tinytext"),
        MYSQL_TYPE_BLOB => pick("blob", "text"),
        MYSQL_TYPE_MEDIUM_BLOB => pick("mediumblob", "mediumtext"),
        MYSQL_TYPE_LONG_BLOB => pick("longblob", "longtext"),
        MYSQL_TYPE_GEOMETRY => "geometry",
        MYSQL_TYPE_VECTOR => "vector",
        MYSQL_TYPE_NULL => "null",
        MYSQL_TYPE_TYPED_ARRAY | MYSQL_TYPE_UNKNOWN => "unknown",
    }
    .to_string()
}

/// How a column takes its parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnKind {
    /// Text the server converts to the column's type.
    Text,
    /// `BINARY`, `VARBINARY` and the blobs: bytes.
    Binary,
    /// `BIT`: a number.
    Bit,
}

/// The kind of a column by its `DATA_TYPE` and character set in `information_schema.columns`.
pub fn kind_of(data_type: &str, charset: Option<&str>) -> ColumnKind {
    match data_type.to_ascii_lowercase().as_str() {
        "bit" => ColumnKind::Bit,
        "binary" | "varbinary" | "tinyblob" | "blob" | "mediumblob" | "longblob" => ColumnKind::Binary,
        "char" | "varchar" | "tinytext" | "text" | "mediumtext" | "longtext" if charset.is_some_and(|c| c.eq_ignore_ascii_case("binary")) => {
            ColumnKind::Binary
        }
        _ => ColumnKind::Text,
    }
}

/// The parameters of the table data, by column (spec 6.2.2). A column that isn't in
/// `kinds` is text.
pub struct MysqlBinder {
    pub kinds: HashMap<String, ColumnKind>,
}

impl MysqlBinder {
    fn kind(&self, column: &str) -> ColumnKind {
        self.kinds.get(column).copied().unwrap_or(ColumnKind::Text)
    }

    /// Text as `column` takes it: the text itself, the bytes of `\x` and hex digits for a
    /// binary column (or of the text when it isn't that), a number for `BIT`.
    fn text_for(&self, column: &str, text: &str) -> Value {
        match self.kind(column) {
            ColumnKind::Text => Value::Bytes(text.as_bytes().to_vec()),
            ColumnKind::Binary => Value::Bytes(unhex(text).unwrap_or_else(|| text.as_bytes().to_vec())),
            ColumnKind::Bit => text.parse::<u64>().map(Value::UInt).unwrap_or_else(|_| Value::Bytes(text.as_bytes().to_vec())),
        }
    }
}

/// The bytes of `\x00ff`, in either case.
fn unhex(text: &str) -> Option<Vec<u8>> {
    let digits = text.strip_prefix("\\x")?;
    if digits.len() % 2 != 0 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..digits.len()).step_by(2).map(|i| u8::from_str_radix(&digits[i..i + 2], 16).ok()).collect()
}

impl ValueBinder for MysqlBinder {
    type Param = Value;

    fn value(&self, column: &str, value: &JsonValue) -> Value {
        match value {
            JsonValue::Null => Value::NULL,
            JsonValue::String(text) => self.text_for(column, text),
            // MySQL reads the text 'true' as 0, so a boolean goes as the number it is
            JsonValue::Bool(b) => self.text_for(column, if *b { "1" } else { "0" }),
            other => self.text_for(column, &other.to_string()),
        }
    }

    fn imported(&self, column: &str, value: Option<&str>) -> Value {
        value.map_or(Value::NULL, |text| self.text_for(column, text))
    }

    fn key(&self, column: &str, value: &JsonValue) -> Vec<Value> {
        vec![self.value(column, value)]
    }
}

impl OperandBinder for MysqlBinder {
    type Param = Value;

    fn text(&self, text: &str) -> Value {
        Value::Bytes(text.as_bytes().to_vec())
    }

    fn compared(&self, column: &str, operand: &str) -> Value {
        self.text_for(column, operand)
    }

    fn matched(&self, column: &str, operand: &str) -> Vec<Value> {
        vec![self.text_for(column, operand)]
    }
}

#[cfg(test)]
mod tests {
    use mysql_async::consts::{ColumnFlags, ColumnType};
    use serde_json::json;

    use super::*;

    /// The binary character set, which every non-text column reports.
    const BINARY: u16 = 63;
    /// utf8mb4_0900_ai_ci
    const UTF8MB4: u16 = 255;

    fn col(column_type: ColumnType) -> Column {
        Column::new(column_type)
    }

    fn text_col(column_type: ColumnType) -> Column {
        Column::new(column_type).with_character_set(UTF8MB4)
    }

    fn bytes(text: &str) -> Value {
        Value::Bytes(text.as_bytes().to_vec())
    }

    #[test]
    fn integers_beyond_javascript_become_text() {
        let unsigned = col(ColumnType::MYSQL_TYPE_LONGLONG).with_flags(ColumnFlags::UNSIGNED_FLAG);
        assert_eq!(value_to_json(&Value::UInt(18446744073709551615), &unsigned), json!("18446744073709551615"));
        assert_eq!(value_to_json(&Value::UInt(7), &unsigned), json!(7));
        let int = col(ColumnType::MYSQL_TYPE_LONG);
        assert_eq!(value_to_json(&Value::Int(42), &int), json!(42));
        assert_eq!(value_to_json(&Value::Int(-9_007_199_254_740_991), &int), json!(-9_007_199_254_740_991_i64));
        let big = col(ColumnType::MYSQL_TYPE_LONGLONG);
        assert_eq!(value_to_json(&Value::Int(i64::MIN), &big), json!("-9223372036854775808"));
        assert_eq!(value_to_json(&bytes("9007199254740993"), &big), json!("9007199254740993"));
        assert_eq!(value_to_json(&bytes("-12"), &big), json!(-12));
        assert_eq!(value_to_json(&bytes("18446744073709551615"), &unsigned), json!("18446744073709551615"));
        assert_eq!(value_to_json(&Value::NULL, &big), JsonValue::Null);
    }

    #[test]
    fn floats_and_years_are_numbers() {
        let float = col(ColumnType::MYSQL_TYPE_FLOAT);
        let double = col(ColumnType::MYSQL_TYPE_DOUBLE);
        assert_eq!(value_to_json(&bytes("1.5"), &float), json!(1.5));
        // The same float in the binary protocol reads as the number the text shows
        assert_eq!(value_to_json(&Value::Float(0.1), &float), json!(0.1));
        assert_eq!(value_to_json(&Value::Double(-2.25), &double), json!(-2.25));
        assert_eq!(value_to_json(&bytes("1.7976931348623157e308"), &double), json!(1.7976931348623157e308));
        let year = col(ColumnType::MYSQL_TYPE_YEAR);
        assert_eq!(value_to_json(&bytes("2024"), &year), json!(2024));
        assert_eq!(value_to_json(&Value::Int(2024), &year), json!(2024));
    }

    #[test]
    fn decimals_dates_and_times_stay_text() {
        assert_eq!(value_to_json(&bytes("12.50"), &col(ColumnType::MYSQL_TYPE_NEWDECIMAL)), json!("12.50"));
        assert_eq!(value_to_json(&bytes("-0.000000000000000001"), &col(ColumnType::MYSQL_TYPE_NEWDECIMAL)), json!("-0.000000000000000001"));

        let zero = Value::Date(0, 0, 0, 0, 0, 0, 0);
        assert_eq!(value_to_json(&zero, &col(ColumnType::MYSQL_TYPE_DATETIME)), json!("0000-00-00 00:00:00"));
        assert_eq!(value_to_json(&zero, &col(ColumnType::MYSQL_TYPE_DATE)), json!("0000-00-00"));
        assert_eq!(value_to_json(&bytes("0000-00-00"), &col(ColumnType::MYSQL_TYPE_DATE)), json!("0000-00-00"));
        let moment = Value::Date(2024, 2, 29, 13, 45, 10, 123456);
        assert_eq!(value_to_json(&moment, &col(ColumnType::MYSQL_TYPE_DATE)), json!("2024-02-29"));
        assert_eq!(value_to_json(&moment, &col(ColumnType::MYSQL_TYPE_TIMESTAMP).with_decimals(6)), json!("2024-02-29 13:45:10.123456"));
        // The column's fraction digits decide, so both protocols show the same text
        assert_eq!(value_to_json(&Value::Date(2024, 2, 29, 13, 45, 10, 0), &col(ColumnType::MYSQL_TYPE_DATETIME).with_decimals(6)), json!("2024-02-29 13:45:10.000000"));
        assert_eq!(value_to_json(&Value::Date(2024, 2, 29, 13, 45, 10, 0), &col(ColumnType::MYSQL_TYPE_DATETIME)), json!("2024-02-29 13:45:10"));
        assert_eq!(value_to_json(&Value::Date(2024, 2, 29, 13, 45, 10, 120000), &col(ColumnType::MYSQL_TYPE_DATETIME).with_decimals(2)), json!("2024-02-29 13:45:10.12"));

        let time = col(ColumnType::MYSQL_TYPE_TIME);
        assert_eq!(value_to_json(&Value::Time(true, 34, 22, 59, 59, 0), &time), json!("-838:59:59"));
        assert_eq!(value_to_json(&Value::Time(false, 0, 1, 2, 3, 0), &time), json!("01:02:03"));
        assert_eq!(value_to_json(&Value::Time(false, 0, 12, 34, 56, 789000), &time.clone().with_decimals(3)), json!("12:34:56.789"));
        assert_eq!(value_to_json(&bytes("-838:59:59"), &time), json!("-838:59:59"));
    }

    #[test]
    fn json_columns_become_json_values() {
        let column = col(ColumnType::MYSQL_TYPE_JSON).with_character_set(BINARY);
        assert_eq!(value_to_json(&bytes(r#"{"a":[1,2]}"#), &column), json!({"a": [1, 2]}));
        assert_eq!(value_to_json(&bytes("null"), &column), JsonValue::Null);
        assert_eq!(value_to_json(&bytes(r#""text""#), &column), json!("text"));
        // Not JSON after all: shown as it is
        assert_eq!(value_to_json(&bytes("{oops"), &column), json!("{oops"));
    }

    #[test]
    fn binary_columns_are_hex_and_text_columns_are_text() {
        let blob = col(ColumnType::MYSQL_TYPE_BLOB).with_flags(ColumnFlags::BLOB_FLAG | ColumnFlags::BINARY_FLAG).with_character_set(BINARY);
        assert_eq!(value_to_json(&Value::Bytes(vec![0x00, 0xff]), &blob), json!("\\x00ff"));
        assert_eq!(value_to_json(&Value::Bytes(vec![]), &blob), json!("\\x"));
        let varbinary = col(ColumnType::MYSQL_TYPE_VAR_STRING).with_character_set(BINARY);
        assert_eq!(value_to_json(&Value::Bytes(vec![0x1a, 0x2B]), &varbinary), json!("\\x1a2b"));
        let varchar = text_col(ColumnType::MYSQL_TYPE_VAR_STRING);
        assert_eq!(value_to_json(&bytes("olá"), &varchar), json!("olá"));
        let text = text_col(ColumnType::MYSQL_TYPE_BLOB).with_flags(ColumnFlags::BLOB_FLAG);
        assert_eq!(value_to_json(&bytes("😀"), &text), json!("😀"));
        let enumeration = text_col(ColumnType::MYSQL_TYPE_STRING).with_flags(ColumnFlags::ENUM_FLAG);
        assert_eq!(value_to_json(&bytes("small"), &enumeration), json!("small"));
        // A number is not binary because its character set says so
        assert_eq!(value_to_json(&bytes("5"), &col(ColumnType::MYSQL_TYPE_LONG).with_character_set(BINARY)), json!(5));
    }

    #[test]
    fn bit_values_are_numbers() {
        let bit8 = col(ColumnType::MYSQL_TYPE_BIT).with_column_length(8).with_character_set(BINARY);
        assert_eq!(value_to_json(&Value::Bytes(vec![5]), &bit8), json!(5));
        let bit64 = col(ColumnType::MYSQL_TYPE_BIT).with_column_length(64).with_character_set(BINARY);
        assert_eq!(value_to_json(&Value::Bytes(vec![0xff; 8]), &bit64), json!("18446744073709551615"));
        assert_eq!(value_to_json(&Value::Bytes(vec![0, 0, 0, 0, 0, 0, 1, 0]), &bit64), json!(256));
        // 2^53 - 1 is the last number JavaScript holds exactly
        assert_eq!(value_to_json(&Value::Bytes(vec![0x00, 0x1f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]), &bit64), json!(9_007_199_254_740_991_u64));
        assert_eq!(value_to_json(&Value::Bytes(vec![0x00, 0x20, 0, 0, 0, 0, 0, 0]), &bit64), json!("9007199254740992"));
        assert_eq!(value_to_json(&Value::Bytes(vec![0x00, 0x20, 0, 0, 0, 0, 0, 1]), &bit64), json!("9007199254740993"));
    }

    #[test]
    fn type_names_are_the_mysql_ones() {
        let t = |column: Column| type_name(&column);
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_TINY)), "tinyint");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_SHORT)), "smallint");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_INT24)), "mediumint");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_LONG).with_flags(ColumnFlags::UNSIGNED_FLAG)), "int");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_LONGLONG)), "bigint");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_NEWDECIMAL)), "decimal");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_FLOAT)), "float");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_DOUBLE)), "double");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_DATE)), "date");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_DATETIME)), "datetime");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_TIMESTAMP)), "timestamp");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_TIME)), "time");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_YEAR)), "year");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_BIT)), "bit");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_JSON).with_character_set(BINARY)), "json");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_BLOB).with_character_set(BINARY)), "blob");
        assert_eq!(t(text_col(ColumnType::MYSQL_TYPE_BLOB)), "text");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_VAR_STRING).with_character_set(BINARY)), "varbinary");
        assert_eq!(t(text_col(ColumnType::MYSQL_TYPE_VAR_STRING)), "varchar");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_STRING).with_character_set(BINARY)), "binary");
        assert_eq!(t(text_col(ColumnType::MYSQL_TYPE_STRING)), "char");
        assert_eq!(t(text_col(ColumnType::MYSQL_TYPE_STRING).with_flags(ColumnFlags::ENUM_FLAG)), "enum");
        assert_eq!(t(text_col(ColumnType::MYSQL_TYPE_STRING).with_flags(ColumnFlags::SET_FLAG)), "set");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_GEOMETRY)), "geometry");
        assert_eq!(t(col(ColumnType::MYSQL_TYPE_NULL)), "null");
    }

    #[test]
    fn kinds_follow_the_data_type_and_charset() {
        assert_eq!(kind_of("bit", None), ColumnKind::Bit);
        for binary in ["binary", "varbinary", "tinyblob", "blob", "mediumblob", "longblob"] {
            assert_eq!(kind_of(binary, None), ColumnKind::Binary, "{binary}");
        }
        for text in ["char", "varchar", "tinytext", "text", "mediumtext", "longtext", "enum", "set", "json", "int", "decimal", "datetime", "point"] {
            assert_eq!(kind_of(text, Some("utf8mb4")), ColumnKind::Text, "{text}");
        }
        assert_eq!(kind_of("INT", None), ColumnKind::Text);
        assert_eq!(kind_of("BLOB", None), ColumnKind::Binary);
        assert_eq!(kind_of("varchar", Some("binary")), ColumnKind::Binary);
    }

    fn binder() -> MysqlBinder {
        MysqlBinder {
            kinds: HashMap::from([("b".to_string(), ColumnKind::Binary), ("f".to_string(), ColumnKind::Bit), ("t".to_string(), ColumnKind::Text)]),
        }
    }

    #[test]
    fn the_binder_sends_binary_as_bytes_and_bit_as_a_number() {
        let binder = binder();
        assert_eq!(binder.value("b", &json!("\\x00ff")), Value::Bytes(vec![0, 255]));
        assert_eq!(binder.value("b", &json!("\\x00FF")), Value::Bytes(vec![0, 255]));
        assert_eq!(binder.value("b", &json!("\\x")), Value::Bytes(vec![]));
        // Text without the prefix, or with digits that aren't hex, is the bytes of the text
        assert_eq!(binder.value("b", &json!("abc")), bytes("abc"));
        assert_eq!(binder.value("b", &json!("\\xzz")), bytes("\\xzz"));
        assert_eq!(binder.value("b", &json!("\\x0")), bytes("\\x0"));
        assert_eq!(binder.value("f", &json!("5")), Value::UInt(5));
        assert_eq!(binder.value("f", &json!(5)), Value::UInt(5));
        assert_eq!(binder.value("f", &json!("18446744073709551615")), Value::UInt(u64::MAX));
        assert_eq!(binder.value("f", &json!(true)), Value::UInt(1));
        assert_eq!(binder.value("f", &json!("x")), bytes("x"));
        assert_eq!(binder.key("t", &json!(1)), vec![bytes("1")]);
        assert_eq!(binder.key("b", &json!("\\x00ff")), vec![Value::Bytes(vec![0, 255])]);
        assert_eq!(binder.key("f", &json!("7")), vec![Value::UInt(7)]);
    }

    #[test]
    fn other_values_go_as_text_and_a_missing_column_is_text() {
        let binder = binder();
        assert_eq!(binder.value("t", &json!("olá")), bytes("olá"));
        assert_eq!(binder.value("t", &json!(2.5)), bytes("2.5"));
        assert_eq!(binder.value("t", &json!("9007199254740993")), bytes("9007199254740993"));
        assert_eq!(binder.value("t", &json!(true)), bytes("1"));
        assert_eq!(binder.value("t", &json!(false)), bytes("0"));
        assert_eq!(binder.value("t", &json!({"a": [1, 2]})), bytes(r#"{"a":[1,2]}"#));
        assert_eq!(binder.value("t", &JsonValue::Null), Value::NULL);
        assert_eq!(binder.value("b", &JsonValue::Null), Value::NULL);
        assert_eq!(binder.value("f", &JsonValue::Null), Value::NULL);
        assert_eq!(binder.value("unknown", &json!("\\x00ff")), bytes("\\x00ff"));
        assert_eq!(binder.key("unknown", &json!(3)), vec![bytes("3")]);
        assert_eq!(binder.imported("t", Some("a,b")), bytes("a,b"));
        assert_eq!(binder.imported("t", None), Value::NULL);
        assert_eq!(binder.imported("b", Some("\\xff")), Value::Bytes(vec![255]));
        assert_eq!(binder.imported("f", Some("3")), Value::UInt(3));
        assert_eq!(binder.imported("f", None), Value::NULL);
    }

    #[test]
    fn filter_operands_follow_the_same_rules() {
        let binder = binder();
        assert_eq!(binder.text("a%"), bytes("a%"));
        assert_eq!(binder.compared("t", "9"), bytes("9"));
        assert_eq!(binder.compared("b", "\\x00ff"), Value::Bytes(vec![0, 255]));
        assert_eq!(binder.compared("f", "5"), Value::UInt(5));
        assert_eq!(binder.matched("t", "x"), vec![bytes("x")]);
        assert_eq!(binder.matched("b", "\\x00"), vec![Value::Bytes(vec![0])]);
        assert_eq!(binder.matched("f", "6"), vec![Value::UInt(6)]);
        assert_eq!(binder.matched("nope", "6"), vec![bytes("6")]);
    }
}

/// Tests that need a MySQL server; see `db/mysql/mod.rs` for how to run them.
#[cfg(test)]
mod server_tests {
    use mysql_async::prelude::Queryable;
    use mysql_async::{Conn, Params, Row};
    use serde_json::{json, Map};

    use super::*;
    use crate::db::mysql_common::tests_support::{exec, raw_conn, TestDb};
    use crate::db::sql::dialect::Dialect;
    use crate::db::sql::edit::build_insert_in;

    type JsonRow = Map<String, JsonValue>;

    /// Both rows of `fixtures/all_types.sql` as the grid shows them.
    fn expected() -> Vec<JsonRow> {
        let rows = json!([
            {
                "id": 1, "c_tinyint": -5, "c_tinyint_u": 200, "c_smallint": 300, "c_mediumint": -70000,
                "c_int": 123456, "c_int_u": 4000000000_u64, "c_bigint": 42, "c_bigint_u": 7,
                "c_decimal": "12.5000", "c_float": 1.5, "c_double": -2.25,
                "c_date": "2024-02-29", "c_datetime": "2024-02-29 13:45:10.123456",
                "c_timestamp": "2024-02-29 13:45:10", "c_time": "12:34:56.789", "c_year": 2024,
                "c_bit8": 5, "c_bit64": 5, "c_json": {"a": [1, 2]},
                "c_binary": "\\x01020300", "c_varbinary": "\\x61", "c_blob": "\\x00ff",
                "c_char": "abc", "c_varchar": "hello", "c_text": "some text",
                "c_enum": "small", "c_set": "a,c", "c_geometry": "POINT(1 2)", "c_null": null
            },
            {
                "id": 2, "c_tinyint": -128, "c_tinyint_u": 255, "c_smallint": -32768, "c_mediumint": 8388607,
                "c_int": -2147483648_i64, "c_int_u": 4294967295_u64,
                "c_bigint": "-9223372036854775808", "c_bigint_u": "18446744073709551615",
                "c_decimal": "-9999999999999999.9999", "c_float": -1e10, "c_double": 1.7976931348623157e308,
                "c_date": "0000-00-00", "c_datetime": "0000-00-00 00:00:00.000000",
                "c_timestamp": "2038-01-19 03:14:07", "c_time": "-838:59:59.000", "c_year": 2155,
                "c_bit8": 255, "c_bit64": "18446744073709551615",
                "c_json": {"a": {"b": [1, {"c": null}], "d": "x"}, "e": true, "f": "9007199254740993"},
                "c_binary": "\\x00ff00ff", "c_varbinary": "\\x", "c_blob": "\\x00ff",
                "c_char": "olá", "c_varchar": "it's \"q\" \\ ç 日本", "c_text": "😀 text",
                "c_enum": "large", "c_set": "", "c_geometry": "POINT(-3.5 0)", "c_null": null
            }
        ]);
        rows.as_array().unwrap().iter().map(|r| r.as_object().unwrap().clone()).collect()
    }

    /// The columns of `table` in order: name, DATA_TYPE and character set.
    async fn columns_of(conn: &mut Conn, table: &str) -> Vec<(String, String, Option<String>)> {
        conn.exec(
            "SELECT column_name, data_type, character_set_name FROM information_schema.columns \
             WHERE table_schema = DATABASE() AND table_name = ? ORDER BY ordinal_position",
            (table,),
        )
        .await
        .unwrap()
    }

    /// Every row of `table` ordered by id, through the text protocol or the binary one.
    /// Geometry comes as WKT (`ST_AsText`), which is how the grid reads it.
    async fn read(conn: &mut Conn, table: &str, columns: &[(String, String, Option<String>)], binary: bool) -> Vec<JsonRow> {
        let list: Vec<String> = columns
            .iter()
            .map(|(name, data_type, _)| if data_type == "point" { format!("ST_AsText(`{name}`) AS `{name}`") } else { format!("`{name}`") })
            .collect();
        let sql = format!("SELECT {} FROM `{table}` ORDER BY id", list.join(", "));
        let rows: Vec<Row> = if binary { conn.exec(sql, ()).await.unwrap() } else { conn.query(sql).await.unwrap() };
        rows.iter()
            .map(|row| {
                row.columns_ref()
                    .iter()
                    .enumerate()
                    .map(|(i, column)| (column.name_str().into_owned(), value_to_json(row.as_ref(i).unwrap(), column)))
                    .collect()
            })
            .collect()
    }

    fn assert_rows(actual: &[JsonRow], expected: &[JsonRow], what: &str) {
        assert_eq!(actual.len(), expected.len(), "{what}: row count");
        for (a, e) in actual.iter().zip(expected) {
            for (name, value) in e {
                assert_eq!(a.get(name), Some(value), "{what}: row {} column {name}", e["id"]);
            }
            assert_eq!(a.len(), e.len(), "{what}: columns");
        }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn every_type_round_trips_through_the_grid() {
        let db = TestDb::new().await;
        exec(&db.endpoint, include_str!("fixtures/all_types.sql")).await;
        let mut conn = raw_conn(&db.endpoint).await;
        let columns = columns_of(&mut conn, "all_types").await;
        assert_eq!(columns.len(), expected()[0].len());

        // Both protocols show the same values
        for binary in [false, true] {
            let rows = read(&mut conn, "all_types", &columns, binary).await;
            assert_rows(&rows, &expected(), if binary { "binary protocol" } else { "text protocol" });
        }

        // The type names of the result columns
        let names: Vec<(String, String)> = conn
            .query::<Row, _>("SELECT * FROM all_types LIMIT 1")
            .await
            .unwrap()[0]
            .columns_ref()
            .iter()
            .map(|c| (c.name_str().into_owned(), type_name(c)))
            .collect();
        let name_of = |column: &str| names.iter().find(|(n, _)| n == column).unwrap().1.clone();
        for (column, type_name) in [
            ("c_tinyint_u", "tinyint"), ("c_smallint", "smallint"), ("c_mediumint", "mediumint"), ("c_int_u", "int"),
            ("c_bigint_u", "bigint"), ("c_decimal", "decimal"), ("c_float", "float"), ("c_double", "double"),
            ("c_date", "date"), ("c_datetime", "datetime"), ("c_timestamp", "timestamp"), ("c_time", "time"),
            ("c_year", "year"), ("c_bit64", "bit"), ("c_json", "json"), ("c_binary", "binary"),
            ("c_varbinary", "varbinary"), ("c_blob", "blob"), ("c_char", "char"), ("c_varchar", "varchar"),
            ("c_text", "text"), ("c_enum", "enum"), ("c_set", "set"), ("c_geometry", "geometry"),
        ] {
            assert_eq!(name_of(column), type_name, "{column}");
        }

        // Each value written back with the binder reads equal. Geometry is read-only in the
        // grid, so it isn't written.
        let kinds: HashMap<String, ColumnKind> = columns.iter().map(|(name, data_type, charset)| (name.clone(), kind_of(data_type, charset.as_deref()))).collect();
        let binder = MysqlBinder { kinds };
        let valid: Vec<String> = columns.iter().map(|(name, ..)| name.clone()).collect();
        conn.query_drop("CREATE TABLE all_types_copy LIKE all_types").await.unwrap();
        conn.query_drop("SET SESSION sql_mode = ''").await.unwrap();
        let mut written = expected();
        for row in &mut written {
            row.remove("c_geometry");
            let (sql, params) = build_insert_in(Dialect::Mysql, "`all_types_copy`", row, &valid, &binder).unwrap();
            conn.exec_drop(sql, Params::Positional(params)).await.unwrap();
        }
        let mut expected_copy = expected();
        for row in &mut expected_copy {
            row.insert("c_geometry".into(), JsonValue::Null);
        }
        for binary in [false, true] {
            let rows = read(&mut conn, "all_types_copy", &columns, binary).await;
            assert_rows(&rows, &expected_copy, if binary { "copy, binary protocol" } else { "copy, text protocol" });
        }

        // A row is found again by its key, with the value the grid showed
        for row in expected() {
            for (name, value) in &row {
                if name == "c_geometry" || value.is_null() || value.is_object() {
                    continue;
                }
                let key: Vec<Value> = binder.key(name, value);
                let sql = format!("SELECT COUNT(*) FROM all_types WHERE `{name}` IN ({})", vec!["?"; key.len()].join(", "));
                let found: i64 = conn.exec_first(sql, Params::Positional(key)).await.unwrap().unwrap();
                assert!(found >= 1, "row {} column {name} isn't found by {value}", row["id"]);
            }
        }
        let _ = conn.disconnect().await;
        db.drop_now().await;
    }
}
