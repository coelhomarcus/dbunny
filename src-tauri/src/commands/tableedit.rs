//! Statements that write rows of a table.
//!
//! Like the filters, values travel as text-format parameters, so the server converts
//! them to each column's type and nothing the user types becomes SQL.

use serde_json::Value as JsonValue;

use super::tablefilter::{quote_ident, TextParam};

/// Text form of a JSON value for a parameter. Null is a real SQL NULL (None).
pub fn param_for(value: &JsonValue) -> Option<TextParam> {
    match value {
        JsonValue::Null => None,
        JsonValue::String(text) => Some(TextParam(text.clone())),
        other => Some(TextParam(other.to_string())),
    }
}

/// `INSERT` for the given columns, with one parameter each. A column that isn't listed
/// gets its default (or NULL). With no columns at all it inserts a row of defaults.
pub fn build_insert(
    relation: &str,
    values: &serde_json::Map<String, JsonValue>,
    valid_columns: &[String],
) -> Result<(String, Vec<Option<TextParam>>), String> {
    for column in values.keys() {
        if !valid_columns.contains(column) {
            return Err(format!("Invalid column: {column}"));
        }
    }
    if values.is_empty() {
        return Ok((format!("INSERT INTO {relation} DEFAULT VALUES"), vec![]));
    }

    let columns: Vec<String> = values.keys().map(|c| quote_ident(c)).collect();
    let placeholders: Vec<String> = (1..=values.len()).map(|i| format!("${i}")).collect();
    let params = values.values().map(param_for).collect();
    Ok((
        format!(
            "INSERT INTO {relation} ({}) VALUES ({})",
            columns.join(", "),
            placeholders.join(", ")
        ),
        params,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn columns() -> Vec<String> {
        ["id", "name", "note", "we\"ird"].iter().map(|s| s.to_string()).collect()
    }

    fn values(pairs: serde_json::Value) -> serde_json::Map<String, JsonValue> {
        pairs.as_object().unwrap().clone()
    }

    #[test]
    fn values_become_numbered_parameters() {
        let (sql, params) = build_insert("\"public\".\"t\"", &values(json!({"name": "ana", "note": "hi"})), &columns()).unwrap();
        assert_eq!(sql, "INSERT INTO \"public\".\"t\" (\"name\", \"note\") VALUES ($1, $2)");
        assert_eq!(params, vec![Some(TextParam("ana".into())), Some(TextParam("hi".into()))]);
    }

    #[test]
    fn null_is_a_real_null_not_the_text_null() {
        let (_, params) = build_insert("t", &values(json!({"name": null, "note": "NULL"})), &columns()).unwrap();
        assert_eq!(params, vec![None, Some(TextParam("NULL".into()))]);
    }

    #[test]
    fn empty_string_stays_an_empty_string() {
        let (_, params) = build_insert("t", &values(json!({"name": ""})), &columns()).unwrap();
        assert_eq!(params, vec![Some(TextParam(String::new()))]);
    }

    #[test]
    fn numbers_and_booleans_are_sent_as_text() {
        let (_, params) = build_insert("t", &values(json!({"id": 5, "note": true})), &columns()).unwrap();
        assert_eq!(params, vec![Some(TextParam("5".into())), Some(TextParam("true".into()))]);
    }

    #[test]
    fn no_values_inserts_the_defaults() {
        let (sql, params) = build_insert("t", &values(json!({})), &columns()).unwrap();
        assert_eq!(sql, "INSERT INTO t DEFAULT VALUES");
        assert!(params.is_empty());
    }

    #[test]
    fn unknown_columns_are_rejected() {
        let err = build_insert("t", &values(json!({"nope": 1})), &columns()).unwrap_err();
        assert!(err.contains("Invalid column: nope"));
    }

    #[test]
    fn column_names_are_quoted_and_values_never_inlined() {
        let (sql, params) = build_insert("t", &values(json!({"we\"ird": "x'); DROP TABLE t; --"})), &columns()).unwrap();
        assert_eq!(sql, "INSERT INTO t (\"we\"\"ird\") VALUES ($1)");
        assert!(!sql.contains("DROP"));
        assert_eq!(params[0], Some(TextParam("x'); DROP TABLE t; --".into())));
    }
}
