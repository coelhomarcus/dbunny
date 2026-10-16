//! ORDER BY clauses for the table view's sort keys.

use super::Dialect;
use crate::db::model::{SortKey, TableDataParams};

/// The sort keys a page asks for: `sort` when given, else the single `sort_column`.
pub fn sort_keys(params: &TableDataParams) -> Vec<SortKey> {
    match params {
        TableDataParams { sort: Some(keys), .. } => keys.clone(),
        TableDataParams { sort_column: Some(column), sort_direction, .. } => {
            vec![SortKey { column: column.clone(), direction: sort_direction.clone() }]
        }
        _ => vec![],
    }
}

/// ` ORDER BY ...` for the sort keys that name real columns (others are ignored).
pub fn order_clause(keys: &[SortKey], valid_columns: &[String]) -> String {
    order_clause_in(Dialect::Postgres, keys, valid_columns)
}

/// `order_clause` in the given dialect.
pub fn order_clause_in(dialect: Dialect, keys: &[SortKey], valid_columns: &[String]) -> String {
    let parts: Vec<String> = keys
        .iter()
        .filter(|k| valid_columns.contains(&k.column))
        .map(|k| format!("{} {}", dialect.quote_ident(&k.column), if k.direction.as_deref() == Some("desc") { "DESC" } else { "ASC" }))
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!(" ORDER BY {}", parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(column: &str, direction: &str) -> SortKey {
        SortKey { column: column.into(), direction: Some(direction.into()) }
    }

    #[test]
    fn several_sort_keys_keep_their_order_and_skip_unknown_columns() {
        let valid: Vec<String> = ["name", "age", "we\"ird"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            order_clause(&[key("age", "desc"), key("nope; DROP", "asc"), key("name", "asc")], &valid),
            " ORDER BY \"age\" DESC, \"name\" ASC"
        );
        assert_eq!(order_clause(&[key("we\"ird", "sideways")], &valid), " ORDER BY \"we\"\"ird\" ASC");
        assert_eq!(order_clause(&[key("nope", "asc")], &valid), "");
        assert_eq!(order_clause(&[], &valid), "");
    }
}
