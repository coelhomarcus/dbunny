//! Renaming, dropping, emptying and duplicating tables and views.

use mysql_async::prelude::Queryable;

use crate::db::mysql::catalog::{column_definitions, qualified, quote};
use crate::db::mysql_common::driver::ServerDriver;
use crate::db::mysql_common::errors::error_message;
use crate::db::{unsupported, Engine};

/// Whether `kind` is something this module renames or drops: a table or a view.
fn keyword(kind: &str) -> Result<&'static str, String> {
    match kind {
        "table" => Ok("TABLE"),
        "view" => Ok("VIEW"),
        other => Err(format!("Unknown kind \"{other}\"")),
    }
}

fn check_name(new_name: &str) -> Result<(), String> {
    if new_name.trim().is_empty() {
        return Err("The new name can't be empty".into());
    }
    Ok(())
}

/// `RENAME TABLE` renames views too, and moves the keys that point at the table along.
pub(crate) fn rename_sql(schema: &str, name: &str, kind: &str, new_name: &str) -> Result<String, String> {
    keyword(kind)?;
    check_name(new_name)?;
    Ok(format!("RENAME TABLE {} TO {}", qualified(schema, name), qualified(schema, new_name.trim())))
}

pub(crate) fn drop_sql(schema: &str, name: &str, kind: &str, cascade: bool) -> Result<String, String> {
    let keyword = keyword(kind)?;
    if cascade {
        return Err(unsupported(Engine::Mysql, "CASCADE"));
    }
    Ok(format!("DROP {keyword} {}", qualified(schema, name)))
}

/// `TRUNCATE TABLE` always starts the numbering again, so there is no option for it.
pub(crate) fn truncate_sql(schema: &str, name: &str, cascade: bool) -> Result<String, String> {
    if cascade {
        return Err(unsupported(Engine::Mysql, "CASCADE"));
    }
    Ok(format!("TRUNCATE TABLE {}", qualified(schema, name)))
}

/// The statements that copy a table: its structure (indexes and computed columns with it, keys
/// to other tables not), then the rows of `columns` when there are any to copy.
pub(crate) fn duplicate_sql(schema: &str, name: &str, new_name: &str, columns: Option<&[String]>) -> Result<Vec<String>, String> {
    check_name(new_name)?;
    let (source, target) = (qualified(schema, name), qualified(schema, new_name.trim()));
    let mut statements = vec![format!("CREATE TABLE {target} LIKE {source}")];
    if let Some(columns) = columns.filter(|c| !c.is_empty()) {
        let list = columns.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ");
        statements.push(format!("INSERT INTO {target} ({list}) SELECT {list} FROM {source}"));
    }
    Ok(statements)
}

/// Copies a table, and its rows when `with_data`. A copy that fails halfway is dropped again.
pub(crate) async fn duplicate(driver: &ServerDriver, schema: &str, name: &str, new_name: &str, with_data: bool) -> Result<(), String> {
    let mut conn = driver.conn().await?;
    let columns = if with_data {
        // Computed columns can't be written
        let all = column_definitions(&mut conn, schema, name).await?;
        Some(all.into_iter().filter(|c| c.generation_expression.is_none()).map(|c| c.name).collect::<Vec<_>>())
    } else {
        None
    };
    let statements = duplicate_sql(schema, name, new_name, columns.as_deref())?;
    for (index, statement) in statements.iter().enumerate() {
        if let Err(e) = conn.query_drop(statement).await {
            if index > 0 {
                let _ = conn.query_drop(format!("DROP TABLE {}", qualified(schema, new_name.trim()))).await;
            }
            return Err(error_message(&e, driver.endpoint()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_statements_quote_with_backticks() {
        assert_eq!(rename_sql("shop", "a`b", "table", "c").unwrap(), "RENAME TABLE `shop`.`a``b` TO `shop`.`c`");
        assert_eq!(rename_sql("shop", "v", "view", "w").unwrap(), "RENAME TABLE `shop`.`v` TO `shop`.`w`");
        assert_eq!(rename_sql("shop", "t", "function", "u").err().unwrap(), "Unknown kind \"function\"");
        assert_eq!(rename_sql("shop", "t", "table", "  ").err().unwrap(), "The new name can't be empty");

        assert_eq!(drop_sql("shop", "t`x", "table", false).unwrap(), "DROP TABLE `shop`.`t``x`");
        assert_eq!(drop_sql("shop", "v", "view", false).unwrap(), "DROP VIEW `shop`.`v`");
        assert_eq!(drop_sql("shop", "v", "matview", false).err().unwrap(), "Unknown kind \"matview\"");
        assert_eq!(drop_sql("shop", "t", "table", true).err().unwrap(), "CASCADE isn't supported by MySQL");

        assert_eq!(truncate_sql("shop", "t`x", false).unwrap(), "TRUNCATE TABLE `shop`.`t``x`");
        assert_eq!(truncate_sql("shop", "t", true).err().unwrap(), "CASCADE isn't supported by MySQL");
    }

    #[test]
    fn a_duplicate_copies_the_structure_and_the_columns_that_can_be_written() {
        assert_eq!(duplicate_sql("shop", "old", "new", None).unwrap(), ["CREATE TABLE `shop`.`new` LIKE `shop`.`old`"]);
        let columns = ["id".to_string(), "na`me".to_string()];
        assert_eq!(
            duplicate_sql("shop", "old", "new", Some(&columns)).unwrap(),
            [
                "CREATE TABLE `shop`.`new` LIKE `shop`.`old`".to_string(),
                "INSERT INTO `shop`.`new` (`id`, `na``me`) SELECT `id`, `na``me` FROM `shop`.`old`".to_string(),
            ]
        );
        // Nothing to copy when every column is computed
        assert_eq!(duplicate_sql("shop", "old", "new", Some(&[])).unwrap(), ["CREATE TABLE `shop`.`new` LIKE `shop`.`old`"]);
        assert_eq!(duplicate_sql("shop", "old", " ", None).err().unwrap(), "The new name can't be empty");
    }
}

#[cfg(test)]
mod server_tests {
    use mysql_async::prelude::Queryable;
    use serde_json::json;

    use crate::db::mysql_common::tests_support::{exec, query_one, TestDb};
    use crate::db::Driver;

    const SHOP: &str = include_str!("fixtures/shop.sql");

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn rename_truncate_duplicate_and_drop_work() {
        let db = TestDb::new().await;
        exec(&db.endpoint, SHOP).await;
        exec(
            &db.endpoint,
            "INSERT INTO customers (email, name) VALUES ('a@x.com', 'Ana'), ('b@x.com', 'Bob'); \
             INSERT INTO orders (customer_id, total, tax_rate) VALUES (1, 100, 0.10), (2, 50, 0.20)",
        )
        .await;
        let d = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
        let editor = d.schema_editor().unwrap();
        let schema = db.name.as_str();
        let count = |table: &'static str| {
            let endpoint = db.endpoint.clone();
            async move { query_one::<i64>(&endpoint, &format!("SELECT COUNT(*) FROM `{table}`")).await.unwrap() }
        };

        // Duplicate with the rows: the computed column stays computed
        editor.duplicate_table(schema, "orders", "orders_copy", true).await.unwrap();
        assert_eq!(count("orders_copy").await, 2);
        let totals = d.console().unwrap().execute("SELECT total_with_tax FROM orders_copy ORDER BY id").await.unwrap();
        assert_eq!(totals.rows, vec![vec![json!("110.00")], vec![json!("60.00")]]);
        let extra: Option<String> = query_one(
            &db.endpoint,
            &format!("SELECT EXTRA FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = '{schema}' AND TABLE_NAME = 'orders_copy' AND COLUMN_NAME = 'total_with_tax'"),
        )
        .await;
        assert!(extra.unwrap().to_lowercase().contains("generated"));
        // Without the rows it is only the structure
        editor.duplicate_table(schema, "orders", "orders_empty", false).await.unwrap();
        assert_eq!(count("orders_empty").await, 0);
        // A name that is taken is the server's error
        assert!(editor.duplicate_table(schema, "orders", "orders_copy", false).await.is_err());

        // Truncate starts the numbering again; a table a key points at can't be emptied
        editor.truncate_table(schema, "orders_copy", false, false).await.unwrap();
        assert_eq!(count("orders_copy").await, 0);
        exec(&db.endpoint, "INSERT INTO orders_copy (customer_id) VALUES (1)").await;
        assert_eq!(query_one::<i64>(&db.endpoint, "SELECT id FROM orders_copy").await, Some(1));
        let err = editor.truncate_table(schema, "customers", false, false).await.err().unwrap();
        assert!(err.to_lowercase().contains("foreign key"), "{err}");
        assert_eq!(editor.truncate_table(schema, "orders", false, true).await.err().unwrap(), "CASCADE isn't supported by MySQL");

        // Rename a table and a view; the keys follow the table
        editor.rename_relation(schema, "customers", "table", "people").await.unwrap();
        assert_eq!(count("people").await, 2);
        let followed: Option<String> = query_one(
            &db.endpoint,
            &format!("SELECT REFERENCED_TABLE_NAME FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA = '{schema}' AND CONSTRAINT_NAME = 'orders_customer_fk'"),
        )
        .await;
        assert_eq!(followed.as_deref(), Some("people"));
        editor.rename_relation(schema, "big_orders", "view", "large_orders").await.unwrap();
        assert_eq!(d.catalog().relation_kind(schema, "large_orders").await.unwrap().as_deref(), Some("view"));
        assert!(d.catalog().relation_kind(schema, "big_orders").await.unwrap().is_none());

        // Drop
        editor.drop_relation(schema, "large_orders", "view", false).await.unwrap();
        editor.drop_relation(schema, "orders_empty", "table", false).await.unwrap();
        assert!(d.catalog().relation_kind(schema, "orders_empty").await.unwrap().is_none());
        assert_eq!(editor.drop_relation(schema, "orders_copy", "table", true).await.err().unwrap(), "CASCADE isn't supported by MySQL");
        assert_eq!(editor.refresh_materialized_view(schema, "x").await.err().unwrap(), "Materialized views isn't supported by MySQL");
        // The server refuses what a key needs
        let err = editor.drop_relation(schema, "people", "table", false).await.err().unwrap();
        assert!(err.to_lowercase().contains("foreign key"), "{err}");
        let mut conn = d.conn().await.unwrap();
        let tables: Vec<String> = conn.query(format!("SHOW TABLES FROM `{schema}`")).await.unwrap();
        assert!(tables.contains(&"people".to_string()) && tables.contains(&"orders".to_string()));
    }
}
