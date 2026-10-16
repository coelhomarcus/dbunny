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

fn check_columns<'a>(columns: impl Iterator<Item = &'a String>, valid_columns: &[String]) -> Result<(), String> {
    for column in columns {
        if !valid_columns.contains(column) {
            return Err(format!("Invalid column: {column}"));
        }
    }
    Ok(())
}

/// `col = $n AND ...` for a primary key, numbering parameters from `first`.
fn key_condition(key: &serde_json::Map<String, JsonValue>, first: usize, params: &mut Vec<Option<TextParam>>) -> String {
    key.iter()
        .enumerate()
        .map(|(i, (column, value))| {
            params.push(param_for(value));
            format!("{} = ${}", quote_ident(column), first + i)
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// `UPDATE` of one row, found by its primary key.
pub fn build_update(
    relation: &str,
    primary_key: &serde_json::Map<String, JsonValue>,
    updates: &serde_json::Map<String, JsonValue>,
    valid_columns: &[String],
) -> Result<(String, Vec<Option<TextParam>>), String> {
    if primary_key.is_empty() {
        return Err("primaryKey is required".into());
    }
    if updates.is_empty() {
        return Err("updates is required".into());
    }
    check_columns(updates.keys().chain(primary_key.keys()), valid_columns)?;

    let mut params = Vec::new();
    let set: Vec<String> = updates
        .iter()
        .enumerate()
        .map(|(i, (column, value))| {
            params.push(param_for(value));
            format!("{} = ${}", quote_ident(column), i + 1)
        })
        .collect();
    let condition = key_condition(primary_key, params.len() + 1, &mut params);
    Ok((format!("UPDATE {relation} SET {} WHERE {condition}", set.join(", ")), params))
}

/// One `DELETE` for every row, so either all of them go or none does.
pub fn build_delete(
    relation: &str,
    primary_keys: &[serde_json::Map<String, JsonValue>],
    valid_columns: &[String],
) -> Result<(String, Vec<Option<TextParam>>), String> {
    if primary_keys.is_empty() {
        return Err("primaryKeys is required".into());
    }
    let mut params = Vec::new();
    let mut conditions = Vec::new();
    for key in primary_keys {
        if key.is_empty() {
            return Err("primaryKeys can't contain an empty key".into());
        }
        check_columns(key.keys(), valid_columns)?;
        conditions.push(format!("({})", key_condition(key, params.len() + 1, &mut params)));
    }
    Ok((format!("DELETE FROM {relation} WHERE {}", conditions.join(" OR ")), params))
}

/// Parameters Postgres accepts in one statement (the protocol counts them in 16 bits).
const MAX_PARAMS: usize = 65_535;

/// Multi-row `INSERT`s for imported rows, split so none goes over the parameter limit.
/// Each batch also returns the index of its first row, to point errors at a row.
pub fn build_import_batches(
    relation: &str,
    columns: &[String],
    rows: &[Vec<Option<String>>],
    valid_columns: &[String],
) -> Result<Vec<(usize, String, Vec<Option<TextParam>>)>, String> {
    if columns.is_empty() {
        return Err("Choose at least one column to import".into());
    }
    check_columns(columns.iter(), valid_columns)?;
    if let Some((i, row)) = rows.iter().enumerate().find(|(_, r)| r.len() != columns.len()) {
        return Err(format!("Row {} has {} values instead of {}", i + 1, row.len(), columns.len()));
    }
    let names = columns.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ");
    let per_batch = (MAX_PARAMS / columns.len()).clamp(1, 1000);
    Ok(rows
        .chunks(per_batch)
        .enumerate()
        .map(|(batch, chunk)| {
            let mut params = Vec::with_capacity(chunk.len() * columns.len());
            let values: Vec<String> = chunk
                .iter()
                .map(|row| {
                    let placeholders: Vec<String> = row
                        .iter()
                        .map(|value| {
                            params.push(value.clone().map(TextParam));
                            format!("${}", params.len())
                        })
                        .collect();
                    format!("({})", placeholders.join(", "))
                })
                .collect();
            (batch * per_batch, format!("INSERT INTO {relation} ({names}) VALUES {}", values.join(", ")), params)
        })
        .collect())
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

    #[test]
    fn updates_set_values_and_find_the_row_by_key_with_parameters() {
        let (sql, params) = build_update(
            "\"public\".\"t\"",
            &values(json!({"id": 7})),
            &values(json!({"name": "it's", "note": null})),
            &columns(),
        )
        .unwrap();
        assert_eq!(sql, "UPDATE \"public\".\"t\" SET \"name\" = $1, \"note\" = $2 WHERE \"id\" = $3");
        assert_eq!(params, vec![Some(TextParam("it's".into())), None, Some(TextParam("7".into()))]);
    }

    #[test]
    fn updates_reject_unknown_columns_and_empty_input() {
        assert!(build_update("t", &values(json!({"id": 1})), &values(json!({"x; DROP": 1})), &columns()).is_err());
        assert!(build_update("t", &values(json!({"nope": 1})), &values(json!({"name": "a"})), &columns()).is_err());
        assert!(build_update("t", &values(json!({})), &values(json!({"name": "a"})), &columns()).is_err());
        assert!(build_update("t", &values(json!({"id": 1})), &values(json!({})), &columns()).is_err());
    }

    #[test]
    fn deletes_combine_every_key_in_one_statement() {
        let keys = vec![values(json!({"id": 1, "name": "a"})), values(json!({"id": 2, "name": "b"}))];
        let (sql, params) = build_delete("t", &keys, &columns()).unwrap();
        assert_eq!(sql, "DELETE FROM t WHERE (\"id\" = $1 AND \"name\" = $2) OR (\"id\" = $3 AND \"name\" = $4)");
        assert_eq!(params.len(), 4);
        assert_eq!(params[2], Some(TextParam("2".into())));
        assert!(build_delete("t", &[values(json!({}))], &columns()).is_err());
        assert!(build_delete("t", &[values(json!({"we\"ird": 1}))], &columns()).is_ok());
    }

    #[test]
    fn imports_are_split_into_batches_under_the_parameter_limit() {
        let cols: Vec<String> = vec!["name".into(), "note".into()];
        let rows: Vec<Vec<Option<String>>> = (0..2500).map(|i| vec![Some(format!("n{i}")), None]).collect();
        let batches = build_import_batches("t", &cols, &rows, &columns()).unwrap();
        assert_eq!(batches.len(), 3, "1000 rows per batch at most");
        assert_eq!(batches[1].0, 1000);
        assert!(batches[0].1.starts_with("INSERT INTO t (\"name\", \"note\") VALUES ($1, $2), ($3, $4)"));
        assert_eq!(batches[2].2.len(), 1000);
        assert_eq!(batches[0].2[1], None, "missing values are real NULLs");

        let wide: Vec<String> = (0..100).map(|i| format!("c{i}")).collect();
        let wide_rows = vec![vec![Some("x".to_string()); 100]; 1000];
        let batches = build_import_batches("t", &wide, &wide_rows, &wide).unwrap();
        assert!(batches.iter().all(|(_, _, p)| p.len() <= MAX_PARAMS));
    }

    #[test]
    fn imports_check_columns_and_row_lengths() {
        assert!(build_import_batches("t", &[], &[], &columns()).is_err());
        assert!(build_import_batches("t", &["nope".into()], &[vec![None]], &columns()).is_err());
        let err = build_import_batches("t", &["name".into()], &[vec![None], vec![None, None]], &columns()).unwrap_err();
        assert!(err.contains("Row 2"), "{err}");
    }
}
