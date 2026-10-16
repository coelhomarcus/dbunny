//! WHERE clauses for the table view's filters.
//!
//! Values never become SQL text: each one is a parameter made by the caller's binder.
//! PostgreSQL's sends text and the server converts it to the column's type (`age > $1`
//! makes `$1` an integer). That also means a bad value ("abc" for an integer column)
//! fails with the server's own message instead of a broken query.

use serde::Deserialize;

use super::bind::{AsText, OperandBinder};
use super::Dialect;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TableFilter {
    pub column: String,
    pub operator: String,
    #[serde(default)]
    pub value: String,
}

/// A value sent as a text-format parameter, whatever type the server expects.
#[derive(Debug, PartialEq)]
pub struct TextParam(pub String);

/// `"name"`, with embedded quotes doubled.
pub fn quote_ident(name: &str) -> String {
    Dialect::Postgres.quote_ident(name)
}

/// The comma-separated values of an IN filter, trimmed, without empty ones.
fn list_items(list: &str) -> Vec<&str> {
    list.split(',').map(str::trim).filter(|item| !item.is_empty()).collect()
}

/// `{"a","b"}` for the comma-separated values of an IN filter.
fn array_literal(list: &str) -> Option<String> {
    let items: Vec<String> = list_items(list)
        .into_iter()
        .map(|item| format!("\"{}\"", item.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    (!items.is_empty()).then(|| format!("{{{}}}", items.join(",")))
}

/// Adds `values` to `params` and returns their placeholders.
fn push_all<P>(dialect: Dialect, values: Vec<P>, params: &mut Vec<P>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| {
            params.push(value);
            dialect.param(params.len())
        })
        .collect()
}

/// Builds ` WHERE (...)` (with a leading space) and its parameters, or an empty string
/// when there are no filters. `match_any` joins the conditions with OR instead of AND.
pub fn build_where(
    filters: &[TableFilter],
    match_any: bool,
    valid_columns: &[String],
) -> Result<(String, Vec<TextParam>), String> {
    build_where_in(Dialect::Postgres, filters, match_any, valid_columns, &AsText)
}

/// `build_where` in the given dialect, with the operands bound by `binder`.
pub fn build_where_in<B: OperandBinder>(
    dialect: Dialect,
    filters: &[TableFilter],
    match_any: bool,
    valid_columns: &[String],
    binder: &B,
) -> Result<(String, Vec<B::Param>), String> {
    let mut conditions = Vec::new();
    let mut params = Vec::new();

    for filter in filters {
        if !valid_columns.contains(&filter.column) {
            return Err(format!("Invalid column: {}", filter.column));
        }
        let column = dialect.quote_ident(&filter.column);
        let next = dialect.param(params.len() + 1);

        let condition = match filter.operator.as_str() {
            op @ ("=" | "!=") => match push_all(dialect, binder.matched(&filter.column, &filter.value), &mut params).as_slice() {
                [one] => format!("{column} {} {one}", if op == "!=" { "<>" } else { op }),
                several => format!("{column} {} ({})", if op == "!=" { "NOT IN" } else { "IN" }, several.join(", ")),
            },
            op @ ("<" | "<=" | ">" | ">=") => {
                params.push(binder.compared(&filter.column, &filter.value));
                format!("{column} {op} {next}")
            }
            // Pattern matching only makes sense on text, so other types are compared as text
            op @ ("like" | "not like" | "ilike" | "not ilike") => {
                params.push(binder.text(&filter.value));
                let keyword = match dialect {
                    Dialect::Postgres => op.to_uppercase(),
                    // SQLite's LIKE already ignores ASCII case, and it has no ILIKE
                    Dialect::Sqlite => if op.starts_with("not") { "NOT LIKE" } else { "LIKE" }.to_string(),
                };
                format!("{} {keyword} {next}", dialect.as_text(&column))
            }
            op @ ("in" | "not in") => match dialect {
                Dialect::Postgres => {
                    let literal = array_literal(&filter.value)
                        .ok_or_else(|| format!("Enter at least one value for {}", filter.column))?;
                    params.push(binder.text(&literal));
                    if op == "in" {
                        format!("{column} = ANY({next})")
                    } else {
                        format!("{column} <> ALL({next})")
                    }
                }
                // No arrays: one parameter per value (and per form of it)
                Dialect::Sqlite => {
                    let items = list_items(&filter.value);
                    if items.is_empty() {
                        return Err(format!("Enter at least one value for {}", filter.column));
                    }
                    let values = items.into_iter().flat_map(|item| binder.matched(&filter.column, item)).collect();
                    let placeholders = push_all(dialect, values, &mut params);
                    let keyword = if op == "in" { "IN" } else { "NOT IN" };
                    format!("{column} {keyword} ({})", placeholders.join(", "))
                }
            },
            "is null" => format!("{column} IS NULL"),
            "is not null" => format!("{column} IS NOT NULL"),
            other => return Err(format!("Unknown filter operator: {other}")),
        };
        conditions.push(condition);
    }

    if conditions.is_empty() {
        return Ok((String::new(), params));
    }
    let joiner = if match_any { " OR " } else { " AND " };
    Ok((format!(" WHERE ({})", conditions.join(joiner)), params))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(column: &str, operator: &str, value: &str) -> TableFilter {
        TableFilter { column: column.into(), operator: operator.into(), value: value.into() }
    }

    fn columns() -> Vec<String> {
        ["id", "name", "tags", "we\"ird"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_filters_means_no_where() {
        assert_eq!(build_where(&[], false, &columns()).unwrap(), (String::new(), vec![]));
    }

    #[test]
    fn comparison_operators_use_parameters() {
        let (sql, params) = build_where(&[filter("id", ">=", "10")], false, &columns()).unwrap();
        assert_eq!(sql, " WHERE (\"id\" >= $1)");
        assert_eq!(params, vec![TextParam("10".into())]);
    }

    #[test]
    fn not_equal_becomes_the_standard_operator() {
        let (sql, _) = build_where(&[filter("id", "!=", "1")], false, &columns()).unwrap();
        assert_eq!(sql, " WHERE (\"id\" <> $1)");
    }

    #[test]
    fn like_family_compares_the_column_as_text() {
        let (sql, params) = build_where(&[filter("id", "ilike", "1%")], false, &columns()).unwrap();
        assert_eq!(sql, " WHERE (\"id\"::text ILIKE $1)");
        assert_eq!(params[0].0, "1%");
        let (sql, _) = build_where(&[filter("name", "not like", "a%")], false, &columns()).unwrap();
        assert_eq!(sql, " WHERE (\"name\"::text NOT LIKE $1)");
    }

    #[test]
    fn null_checks_take_no_parameter() {
        let (sql, params) = build_where(
            &[filter("name", "is null", ""), filter("id", "is not null", "")],
            false,
            &columns(),
        )
        .unwrap();
        assert_eq!(sql, " WHERE (\"name\" IS NULL AND \"id\" IS NOT NULL)");
        assert!(params.is_empty());
    }

    #[test]
    fn in_builds_an_array_parameter() {
        let (sql, params) = build_where(&[filter("id", "in", "1, 2 ,3")], false, &columns()).unwrap();
        assert_eq!(sql, " WHERE (\"id\" = ANY($1))");
        assert_eq!(params, vec![TextParam("{\"1\",\"2\",\"3\"}".into())]);
        let (sql, _) = build_where(&[filter("id", "not in", "1")], false, &columns()).unwrap();
        assert_eq!(sql, " WHERE (\"id\" <> ALL($1))");
    }

    #[test]
    fn array_elements_escape_quotes_and_backslashes() {
        let (_, params) = build_where(&[filter("name", "in", r#"a"b, c\d"#)], false, &columns()).unwrap();
        assert_eq!(params[0].0, r#"{"a\"b","c\\d"}"#);
    }

    #[test]
    fn an_empty_in_list_is_an_error() {
        let err = build_where(&[filter("id", "in", " , ")], false, &columns()).unwrap_err();
        assert!(err.contains("at least one value"));
    }

    #[test]
    fn conditions_join_with_and_or_or_and_number_parameters_in_order() {
        let filters = [filter("id", ">", "1"), filter("name", "like", "a%"), filter("id", "<", "9")];
        let (all, params) = build_where(&filters, false, &columns()).unwrap();
        assert_eq!(all, " WHERE (\"id\" > $1 AND \"name\"::text LIKE $2 AND \"id\" < $3)");
        assert_eq!(params.len(), 3);
        let (any, _) = build_where(&filters, true, &columns()).unwrap();
        assert_eq!(any, " WHERE (\"id\" > $1 OR \"name\"::text LIKE $2 OR \"id\" < $3)");
    }

    #[test]
    fn parameters_skip_filters_that_have_none() {
        let filters = [filter("name", "is null", ""), filter("id", "=", "5")];
        let (sql, params) = build_where(&filters, false, &columns()).unwrap();
        assert_eq!(sql, " WHERE (\"name\" IS NULL AND \"id\" = $1)");
        assert_eq!(params, vec![TextParam("5".into())]);
    }

    #[test]
    fn unknown_columns_and_operators_are_rejected() {
        assert!(build_where(&[filter("nope", "=", "1")], false, &columns()).unwrap_err().contains("Invalid column"));
        assert!(build_where(&[filter("id", "; drop table x", "1")], false, &columns()).unwrap_err().contains("Unknown filter operator"));
    }

    #[test]
    fn identifiers_with_quotes_are_escaped() {
        let (sql, _) = build_where(&[filter("we\"ird", "=", "1")], false, &columns()).unwrap();
        assert_eq!(sql, " WHERE (\"we\"\"ird\" = $1)");
    }

    #[test]
    fn values_never_appear_in_the_sql() {
        let (sql, params) = build_where(&[filter("name", "=", "x'; DROP TABLE users; --")], false, &columns()).unwrap();
        assert!(!sql.contains("DROP"));
        assert_eq!(params[0].0, "x'; DROP TABLE users; --");
    }

    #[test]
    fn sqlite_filters_use_numbered_question_marks_and_expand_in_lists() {
        let cols: Vec<String> = vec!["id".into(), "name".into()];
        let f = |column: &str, operator: &str, value: &str| TableFilter { column: column.into(), operator: operator.into(), value: value.into() };
        let (sql, params) = build_where_in(Dialect::Sqlite, &[f("id", ">", "3"), f("name", "in", "a, b")], false, &cols, &AsText).unwrap();
        assert_eq!(sql, " WHERE (\"id\" > ?1 AND \"name\" IN (?2, ?3))");
        assert_eq!(params, vec![TextParam("3".into()), TextParam("a".into()), TextParam("b".into())]);
        let (sql, _) = build_where_in(Dialect::Sqlite, &[f("id", "not in", "1")], false, &cols, &AsText).unwrap();
        assert_eq!(sql, " WHERE (\"id\" NOT IN (?1))");
        let err = build_where_in(Dialect::Sqlite, &[f("id", "in", " , ")], false, &cols, &AsText).unwrap_err();
        assert_eq!(err, "Enter at least one value for id");
    }

    #[test]
    fn sqlite_pattern_filters_compare_as_text_with_like() {
        let (sql, _) = build_where_in(Dialect::Sqlite, &[TableFilter { column: "name".into(), operator: "not ilike".into(), value: "%x%".into() }], false, &["name".into()], &AsText).unwrap();
        assert_eq!(sql, " WHERE (CAST(\"name\" AS TEXT) NOT LIKE ?1)");
        let (sql, _) = build_where_in(Dialect::Sqlite, &[filter("name", "ilike", "a%"), filter("name", "like", "b%"), filter("name", "not like", "c%")], false, &columns(), &AsText).unwrap();
        assert_eq!(sql, " WHERE (CAST(\"name\" AS TEXT) LIKE ?1 AND CAST(\"name\" AS TEXT) LIKE ?2 AND CAST(\"name\" AS TEXT) NOT LIKE ?3)");
    }
}
