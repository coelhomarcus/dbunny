//! How the shared builders turn values into parameters.
//!
//! PostgreSQL gets every value as text and converts it to the column's type itself
//! (`AsText`). An engine that doesn't convert for every column (SQLite keeps a value as it
//! comes in a column without a type) passes a binder that picks a parameter per column,
//! and may match a key or a filter value against several forms of it.

use serde_json::Value as JsonValue;

use super::edit::param_for;
use super::filter::TextParam;

/// The parameters of the statements that write rows.
pub trait ValueBinder {
    type Param;
    /// A value written to `column` (`SET`, `INSERT`).
    fn value(&self, column: &str, value: &JsonValue) -> Self::Param;
    /// An imported value for `column`: text, or None for NULL.
    fn imported(&self, column: &str, value: Option<&str>) -> Self::Param;
    /// The forms a key value of `column` may be stored as, at least one; a row whose
    /// column holds any of them matches.
    fn key(&self, column: &str, value: &JsonValue) -> Vec<Self::Param>;
}

/// The parameters of the table view's filters.
pub trait OperandBinder {
    type Param;
    /// Text as it is (patterns, PostgreSQL's array literals).
    fn text(&self, text: &str) -> Self::Param;
    /// The operand of `<`, `<=`, `>` or `>=` on `column`.
    fn compared(&self, column: &str, operand: &str) -> Self::Param;
    /// The forms an operand of `=`, `!=`, IN or NOT IN on `column` matches, at least one.
    fn matched(&self, column: &str, operand: &str) -> Vec<Self::Param>;
}

/// Every value as one text parameter, whatever the column: the server converts it.
pub struct AsText;

impl ValueBinder for AsText {
    type Param = Option<TextParam>;

    fn value(&self, _column: &str, value: &JsonValue) -> Option<TextParam> {
        param_for(value)
    }

    fn imported(&self, _column: &str, value: Option<&str>) -> Option<TextParam> {
        value.map(|v| TextParam(v.to_string()))
    }

    fn key(&self, _column: &str, value: &JsonValue) -> Vec<Option<TextParam>> {
        vec![param_for(value)]
    }
}

impl OperandBinder for AsText {
    type Param = TextParam;

    fn text(&self, text: &str) -> TextParam {
        TextParam(text.to_string())
    }

    fn compared(&self, _column: &str, operand: &str) -> TextParam {
        TextParam(operand.to_string())
    }

    fn matched(&self, _column: &str, operand: &str) -> Vec<TextParam> {
        vec![TextParam(operand.to_string())]
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::edit::{build_delete, build_import_batches, build_insert, build_update};
    use super::super::filter::{build_where, TableFilter};
    use super::*;

    fn text(s: &str) -> Option<TextParam> {
        Some(TextParam(s.into()))
    }

    /// The PostgreSQL statements are the same whatever binder the shared builders take.
    #[test]
    fn postgres_binds_every_value_as_one_text_parameter() {
        let columns: Vec<String> = ["id", "name", "flag", "n"].iter().map(|c| c.to_string()).collect();
        let map = |v: JsonValue| v.as_object().unwrap().clone();

        let (sql, params) = build_insert("t", &map(json!({"flag": true, "n": 2.5, "name": null})), &columns).unwrap();
        assert_eq!(sql, "INSERT INTO t (\"flag\", \"n\", \"name\") VALUES ($1, $2, $3)");
        assert_eq!(params, vec![text("true"), text("2.5"), None]);

        let (sql, params) = build_update("t", &map(json!({"id": "9007199254740993", "n": 1})), &map(json!({"name": "\\x00ff"})), &columns).unwrap();
        assert_eq!(sql, "UPDATE t SET \"name\" = $1 WHERE \"id\" = $2 AND \"n\" = $3");
        assert_eq!(params, vec![text("\\x00ff"), text("9007199254740993"), text("1")]);

        let (sql, params) = build_delete("t", &[map(json!({"id": "1"})), map(json!({"id": null}))], &columns).unwrap();
        assert_eq!(sql, "DELETE FROM t WHERE (\"id\" = $1) OR (\"id\" = $2)");
        assert_eq!(params, vec![text("1"), None]);

        let rows = vec![vec![Some("Infinity".to_string()), None]];
        let batches = build_import_batches("t", &["n".into(), "name".into()], &rows, &columns).unwrap();
        assert_eq!(batches, vec![(0, "INSERT INTO t (\"n\", \"name\") VALUES ($1, $2)".to_string(), vec![text("Infinity"), None])]);

        let f = |column: &str, operator: &str, value: &str| TableFilter { column: column.into(), operator: operator.into(), value: value.into() };
        let filters = [f("id", "=", "1"), f("id", "!=", "\\x00"), f("n", ">", "9"), f("name", "like", "a%"), f("id", "in", "1,2"), f("id", "not in", "3")];
        let (sql, params) = build_where(&filters, false, &columns).unwrap();
        assert_eq!(
            sql,
            " WHERE (\"id\" = $1 AND \"id\" <> $2 AND \"n\" > $3 AND \"name\"::text LIKE $4 AND \"id\" = ANY($5) AND \"id\" <> ALL($6))"
        );
        let params: Vec<&str> = params.iter().map(|p| p.0.as_str()).collect();
        assert_eq!(params, vec!["1", "\\x00", "9", "a%", "{\"1\",\"2\"}", "{\"3\"}"]);
    }
}
