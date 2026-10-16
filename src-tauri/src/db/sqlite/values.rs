//! SQLite values and types in the shapes the app already knows from PostgreSQL.

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
