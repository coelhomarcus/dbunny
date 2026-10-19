//! What the agent is told about the rows a query read: cut to a size it can use, and with
//! nothing it was not meant to see.

use base64::Engine as _;
use serde_json::{json, Value};

use crate::db::model::{QueryColumn, ReadResult};

pub const MAX_RESULT_BYTES: usize = 32 * 1024;
pub const MAX_VALUE_CHARS: usize = 500;

/// Room kept for the closing keys (`truncated`, `note`) when rows are dropped to fit.
const CLOSING_MARGIN: usize = 200;

/// The JSON of a read for the agent: `columns`, `rows`, `rowCount`, `truncated`, a `note` when
/// rows were left out, and `valuesHidden` instead of `rows` when the connection shares no values.
pub fn agent_result(result: &ReadResult, limit: usize, schema_only: bool) -> String {
    let columns: Vec<Value> = result.columns.iter().map(|c| json!({"name": c.name, "type": c.data_type})).collect();
    let shown = result.rows.len().min(limit);
    if schema_only {
        return json!({"columns": columns, "rowCount": shown, "truncated": result.more, "valuesHidden": true}).to_string();
    }

    let mut truncated = result.more || result.rows.len() > limit;
    let head = json!({"columns": columns}).to_string().len();
    let mut size = head + CLOSING_MARGIN;
    let mut rows: Vec<Value> = Vec::new();
    for row in result.rows.iter().take(limit) {
        let row = Value::Array(row.iter().zip(&result.columns).map(|(value, column)| shown_value(value, column)).collect());
        size += row.to_string().len() + 1;
        if size > MAX_RESULT_BYTES {
            truncated = true;
            break;
        }
        rows.push(row);
    }

    let mut out = json!({"columns": columns, "rows": rows, "rowCount": rows.len(), "truncated": truncated});
    if truncated {
        out["note"] = json!(format!("Only the first {} rows are shown.", rows.len()));
    }
    out.to_string()
}

/// A value as the agent sees it: long text cut on a character boundary, binary described.
fn shown_value(value: &Value, column: &QueryColumn) -> Value {
    let Value::String(text) = value else { return value.clone() };
    let kind = column.data_type.to_ascii_lowercase();
    if ["bytea", "blob", "binary"].iter().any(|word| kind.contains(word)) {
        return Value::String(format!("<binary {} bytes>", binary_length(text)));
    }
    match text.char_indices().nth(MAX_VALUE_CHARS) {
        Some((end, _)) => Value::String(format!("{}…", &text[..end])),
        None => value.clone(),
    }
}

/// How many bytes a binary value stands for: `\x…` is hex (SQLite, MySQL), the rest base64 (PostgreSQL).
fn binary_length(text: &str) -> usize {
    if let Some(hex) = text.strip_prefix("\\x") {
        return hex.len() / 2;
    }
    base64::engine::general_purpose::STANDARD.decode(text).map_or(text.len(), |bytes| bytes.len())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::db::model::QueryColumn;

    fn column(name: &str, data_type: &str) -> QueryColumn {
        QueryColumn { name: name.into(), data_type: data_type.into() }
    }

    fn read(columns: Vec<QueryColumn>, rows: Vec<Vec<Value>>, more: bool) -> ReadResult {
        ReadResult { columns, rows, more }
    }

    fn parsed(text: &str) -> Value {
        serde_json::from_str(text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
    }

    #[test]
    fn small_result_goes_whole() {
        let result = read(vec![column("id", "int4"), column("name", "text")], vec![vec![json!(1), json!("a")], vec![json!(2), json!("b")], vec![json!(3), Value::Null]], false);
        let out = parsed(&agent_result(&result, 100, false));
        assert_eq!(out["rowCount"], 3);
        assert_eq!(out["truncated"], false);
        assert!(out.get("note").is_none());
        assert_eq!(out["columns"][0], json!({"name": "id", "type": "int4"}));
        assert_eq!(out["rows"][2], json!([3, null]));
    }

    #[test]
    fn more_rows_are_cut() {
        let rows = (0..100).map(|i| vec![json!(i)]).collect();
        let out = parsed(&agent_result(&read(vec![column("n", "int4")], rows, true), 100, false));
        assert_eq!(out["truncated"], true);
        assert_eq!(out["note"], "Only the first 100 rows are shown.");
        assert_eq!(out["rowCount"], 100);
    }

    #[test]
    fn long_text_is_cut() {
        let out = parsed(&agent_result(&read(vec![column("t", "text")], vec![vec![json!("x".repeat(2000))]], false), 100, false));
        let text = out["rows"][0][0].as_str().unwrap();
        assert_eq!(text.chars().count(), MAX_VALUE_CHARS + 1);
        assert!(text.ends_with('…'));
    }

    #[test]
    fn cuts_on_char_boundaries() {
        for unit in ["é", "🐇", "日"] {
            let out = parsed(&agent_result(&read(vec![column("t", "text")], vec![vec![json!(unit.repeat(600))]], false), 100, false));
            let text = out["rows"][0][0].as_str().unwrap();
            assert_eq!(text.chars().filter(|c| c.to_string() == unit).count(), MAX_VALUE_CHARS, "{unit}");
        }
        // Rows of wide characters past 32 KB are dropped from the end, and the JSON stays valid
        let rows = (0..100).map(|_| vec![json!("日".repeat(400))]).collect();
        let text = agent_result(&read(vec![column("t", "text")], rows, false), 100, false);
        assert!(text.len() <= MAX_RESULT_BYTES, "{}", text.len());
        let out = parsed(&text);
        assert_eq!(out["truncated"], true);
        assert!(out["rows"].as_array().unwrap().len() < 100);
    }

    #[test]
    fn bytes_cap_drops_rows_from_the_end() {
        let rows = (0..100).map(|i| vec![json!(format!("{i}-{}", "y".repeat(450)))]).collect();
        let text = agent_result(&read(vec![column("t", "text")], rows, false), 100, false);
        assert!(text.len() <= MAX_RESULT_BYTES);
        let out = parsed(&text);
        assert_eq!(out["truncated"], true);
        assert!(out["rows"][0][0].as_str().unwrap().starts_with("0-"));
        assert_eq!(out["rowCount"], out["rows"].as_array().unwrap().len());
    }

    #[test]
    fn binary_is_described() {
        let blob = format!("\\x{}", "ab".repeat(16));
        let out = parsed(&agent_result(&read(vec![column("b", "blob"), column("c", "bytea")], vec![vec![json!(blob), json!("AAECAw==")]], false), 100, false));
        assert_eq!(out["rows"][0], json!(["<binary 16 bytes>", "<binary 4 bytes>"]));
    }

    #[test]
    fn schema_only_hides_values() {
        let result = read(vec![column("id", "int4")], vec![vec![json!(1)], vec![json!(2)], vec![json!(3)]], false);
        let out = parsed(&agent_result(&result, 100, true));
        assert_eq!(out, json!({"columns": [{"name": "id", "type": "int4"}], "rowCount": 3, "truncated": false, "valuesHidden": true}));
    }
}
