//! Conversion of values read through PostgreSQL's text protocol.
//!
//! The binary protocol is fast, but each type needs its own decoder. For columns
//! whose type the driver can't decode (enums, intervals, network addresses, money,
//! ranges, domains...) the statement is run through the text protocol instead,
//! where every value arrives as readable text, and converted back to JSON here.

use base64::Engine;
use serde_json::Value as JsonValue;
use tokio_postgres::types::{Kind, Type};

use crate::db::sql::json::int_to_json;

/// Types whose binary values are decoded directly into JSON by the query code.
pub fn is_native(ty: &Type) -> bool {
    matches!(
        *ty,
        Type::BOOL
            | Type::INT2
            | Type::INT4
            | Type::OID
            | Type::INT8
            | Type::FLOAT4
            | Type::FLOAT8
            | Type::NUMERIC
            | Type::JSON
            | Type::JSONB
            | Type::DATE
            | Type::TIME
            | Type::TIMESTAMP
            | Type::TIMESTAMPTZ
            | Type::UUID
            | Type::BYTEA
            | Type::TEXT
            | Type::VARCHAR
            | Type::BPCHAR
            | Type::NAME
            | Type::BOOL_ARRAY
            | Type::INT2_ARRAY
            | Type::INT4_ARRAY
            | Type::OID_ARRAY
            | Type::INT8_ARRAY
            | Type::FLOAT4_ARRAY
            | Type::FLOAT8_ARRAY
            | Type::NUMERIC_ARRAY
            | Type::TEXT_ARRAY
            | Type::VARCHAR_ARRAY
            | Type::BPCHAR_ARRAY
            | Type::NAME_ARRAY
            | Type::UUID_ARRAY
            | Type::DATE_ARRAY
            | Type::TIMESTAMP_ARRAY
            | Type::TIMESTAMPTZ_ARRAY
    )
}

/// Converts the text form of a value of type `ty` into JSON.
pub fn text_to_json(ty: &Type, text: &str) -> JsonValue {
    match ty.kind() {
        Kind::Array(element) => {
            return parse_array(text, element).unwrap_or_else(|| JsonValue::String(text.to_string()));
        }
        // A domain is stored and printed like its base type
        Kind::Domain(base) => return text_to_json(base, text),
        _ => {}
    }

    match *ty {
        Type::BOOL => match text {
            "t" | "true" => JsonValue::Bool(true),
            "f" | "false" => JsonValue::Bool(false),
            _ => JsonValue::String(text.to_string()),
        },
        Type::INT2 | Type::INT4 | Type::OID | Type::INT8 => text
            .parse::<i64>()
            .map(int_to_json)
            .unwrap_or_else(|_| JsonValue::String(text.to_string())),
        Type::FLOAT4 | Type::FLOAT8 => text
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(JsonValue::Number)
            // NaN and Infinity have no JSON number form
            .unwrap_or_else(|| JsonValue::String(text.to_string())),
        Type::JSON | Type::JSONB => {
            serde_json::from_str(text).unwrap_or_else(|_| JsonValue::String(text.to_string()))
        }
        Type::BYTEA => match text.strip_prefix("\\x").and_then(decode_hex) {
            // Same encoding as the binary path
            Some(bytes) => JsonValue::String(base64::engine::general_purpose::STANDARD.encode(bytes)),
            None => JsonValue::String(text.to_string()),
        },
        _ => JsonValue::String(text.to_string()),
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Parses an array literal such as `{1,2,3}`, `{"a b",NULL}` or `{{1,2},{3,4}}`.
fn parse_array(text: &str, element: &Type) -> Option<JsonValue> {
    // Arrays with custom bounds are printed as `[0:2]={...}`
    let literal = if text.starts_with('[') { text.split_once('=')?.1 } else { text };
    let chars: Vec<char> = literal.chars().collect();
    let mut pos = 0;
    let value = parse_array_level(&chars, &mut pos, element)?;
    (pos == chars.len()).then_some(value)
}

fn parse_array_level(chars: &[char], pos: &mut usize, element: &Type) -> Option<JsonValue> {
    if *chars.get(*pos)? != '{' {
        return None;
    }
    *pos += 1;

    let mut items = Vec::new();
    loop {
        match *chars.get(*pos)? {
            '}' => {
                *pos += 1;
                return Some(JsonValue::Array(items));
            }
            ',' => *pos += 1,
            '{' => items.push(parse_array_level(chars, pos, element)?),
            '"' => {
                *pos += 1;
                let mut value = String::new();
                loop {
                    match *chars.get(*pos)? {
                        '\\' => {
                            *pos += 1;
                            value.push(*chars.get(*pos)?);
                            *pos += 1;
                        }
                        '"' => {
                            *pos += 1;
                            break;
                        }
                        c => {
                            value.push(c);
                            *pos += 1;
                        }
                    }
                }
                // A quoted NULL is the text "NULL", not a null
                items.push(text_to_json(element, &value));
            }
            _ => {
                let start = *pos;
                while !matches!(*chars.get(*pos)?, ',' | '}') {
                    *pos += 1;
                }
                let raw: String = chars[start..*pos].iter().collect();
                if raw.eq_ignore_ascii_case("null") {
                    items.push(JsonValue::Null);
                } else {
                    items.push(text_to_json(element, &raw));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scalars_are_converted_by_type() {
        assert_eq!(text_to_json(&Type::BOOL, "t"), json!(true));
        assert_eq!(text_to_json(&Type::BOOL, "f"), json!(false));
        assert_eq!(text_to_json(&Type::INT4, "-42"), json!(-42));
        assert_eq!(text_to_json(&Type::INT8, "9007199254740991"), json!(9007199254740991_i64));
        // Past what JavaScript holds exactly, the digits travel as text
        assert_eq!(text_to_json(&Type::INT8, "9007199254740993"), json!("9007199254740993"));
        assert_eq!(text_to_json(&Type::INT8, "-9223372036854775808"), json!("-9223372036854775808"));
        assert_eq!(text_to_json(&Type::FLOAT8, "1.5"), json!(1.5));
        assert_eq!(text_to_json(&Type::FLOAT8, "NaN"), json!("NaN"));
        assert_eq!(text_to_json(&Type::FLOAT8, "Infinity"), json!("Infinity"));
        assert_eq!(text_to_json(&Type::JSONB, r#"{"a": [1, 2]}"#), json!({"a": [1, 2]}));
        assert_eq!(text_to_json(&Type::JSONB, "not json"), json!("not json"));
    }

    #[test]
    fn types_without_a_json_form_stay_as_text() {
        assert_eq!(text_to_json(&Type::INTERVAL, "1 day 02:00:00"), json!("1 day 02:00:00"));
        assert_eq!(text_to_json(&Type::INET, "10.0.0.1/24"), json!("10.0.0.1/24"));
        assert_eq!(text_to_json(&Type::MONEY, "$1,234.50"), json!("$1,234.50"));
        assert_eq!(text_to_json(&Type::TIMETZ, "10:00:00+02"), json!("10:00:00+02"));
        assert_eq!(text_to_json(&Type::NUMERIC, "12345678901234567890.123"), json!("12345678901234567890.123"));
        assert_eq!(text_to_json(&Type::INT4_RANGE, "[1,10)"), json!("[1,10)"));
    }

    #[test]
    fn bytea_hex_becomes_base64_like_the_binary_path() {
        assert_eq!(text_to_json(&Type::BYTEA, "\\x68656c6c6f"), json!("aGVsbG8="));
        assert_eq!(text_to_json(&Type::BYTEA, "\\xzz"), json!("\\xzz"));
        assert_eq!(text_to_json(&Type::BYTEA, "\\x"), json!(""));
    }

    #[test]
    fn arrays_are_parsed_with_their_element_type() {
        assert_eq!(text_to_json(&Type::INT4_ARRAY, "{1,2,3}"), json!([1, 2, 3]));
        assert_eq!(text_to_json(&Type::INT4_ARRAY, "{}"), json!([]));
        assert_eq!(text_to_json(&Type::INT4_ARRAY, "{1,NULL,3}"), json!([1, null, 3]));
        assert_eq!(text_to_json(&Type::BOOL_ARRAY, "{t,f}"), json!([true, false]));
        assert_eq!(text_to_json(&Type::INTERVAL_ARRAY, "{\"1 day\",\"2 days\"}"), json!(["1 day", "2 days"]));
    }

    #[test]
    fn quoted_array_elements_keep_commas_quotes_and_backslashes() {
        assert_eq!(
            text_to_json(&Type::TEXT_ARRAY, r#"{"a, b","say \"hi\"","back\\slash"}"#),
            json!(["a, b", "say \"hi\"", "back\\slash"])
        );
    }

    #[test]
    fn quoted_null_is_text_but_bare_null_is_null() {
        assert_eq!(text_to_json(&Type::TEXT_ARRAY, r#"{"NULL",NULL}"#), json!(["NULL", null]));
    }

    #[test]
    fn multi_dimensional_arrays_nest() {
        assert_eq!(text_to_json(&Type::INT4_ARRAY, "{{1,2},{3,4}}"), json!([[1, 2], [3, 4]]));
    }

    #[test]
    fn arrays_with_custom_bounds_are_supported() {
        assert_eq!(text_to_json(&Type::INT4_ARRAY, "[0:2]={7,8,9}"), json!([7, 8, 9]));
    }

    #[test]
    fn malformed_arrays_fall_back_to_the_text() {
        assert_eq!(text_to_json(&Type::INT4_ARRAY, "{1,2"), json!("{1,2"));
        assert_eq!(text_to_json(&Type::INT4_ARRAY, "{1,2}}"), json!("{1,2}}"));
        assert_eq!(text_to_json(&Type::INT4_ARRAY, "nope"), json!("nope"));
    }

    #[test]
    fn native_types_are_left_to_the_binary_decoders() {
        assert!(is_native(&Type::INT4));
        assert!(is_native(&Type::TEXT_ARRAY));
        assert!(is_native(&Type::TIMESTAMPTZ));
        assert!(!is_native(&Type::INTERVAL));
        assert!(!is_native(&Type::INET));
        assert!(!is_native(&Type::MONEY));
        // TIMETZ used to be shown as <binary>: the driver can't read it as a plain time
        assert!(!is_native(&Type::TIMETZ));
        assert!(!is_native(&Type::INT4_RANGE));
    }
}
