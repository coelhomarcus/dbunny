//! Query plans: turns the JSON of `EXPLAIN FORMAT=JSON` / `ANALYZE FORMAT=JSON` into the nodes
//! the plan view already draws for MySQL.

use serde_json::{Map, Value as JsonValue};

use crate::db::mysql_common::plan::Node;

/// Keys whose objects describe the plan's own bookkeeping, not a step of it.
const NOT_STEPS: [&str; 2] = ["r_engine_stats", "query_optimization"];

/// The nodes of a plan as `[{ label, cost, rows, actualTimeFirst, actualTimeLast, actualRows,
/// loops, children }]`; what the plan doesn't say is null. `json` is what the server returned:
/// `{ "query_block": { ... } }`.
pub(crate) fn tree_from_json(json: &JsonValue) -> Result<JsonValue, String> {
    let block = json.get("query_block").ok_or("The server returned a plan DBunny can't read")?;
    let mut roots = nodes_for("query_block", block);
    if roots.is_empty() {
        return Err("The server returned a plan DBunny can't read".into());
    }
    // The block itself doesn't say how many rows came out; the step on top does
    for root in &mut roots {
        if root.actual_rows.is_none() {
            root.actual_rows = first_steps(&root.children);
        }
    }
    Ok(JsonValue::Array(roots.drain(..).map(|node| node.to_json()).collect()))
}

/// The rows the first step of `steps` measured, or the first below it that did.
fn first_steps(steps: &[Node]) -> Option<f64> {
    let first = steps.first()?;
    first.actual_rows.or_else(|| first_steps(&first.children))
}

/// The steps `value` stands for under `key`: an object is one step, a list of objects is the
/// steps of each.
fn nodes_for(key: &str, value: &JsonValue) -> Vec<Node> {
    match value {
        JsonValue::Object(object) => vec![node_of(key, object)],
        JsonValue::Array(items) if items.iter().all(JsonValue::is_object) && !items.is_empty() => {
            let steps: Vec<Node> = items.iter().flat_map(|item| item.as_object().into_iter().flat_map(children_of)).collect();
            if key == "nested_loop" {
                vec![Node { label: "Nested loop".into(), children: steps, ..Default::default() }]
            } else {
                steps
            }
        }
        _ => Vec::new(),
    }
}

/// The steps below an object: those of its values that are objects or lists of objects. The
/// keys come sorted, which would put the join buffer before the table it joins to, so a table
/// goes first.
fn children_of(object: &Map<String, JsonValue>) -> Vec<Node> {
    let mut entries: Vec<(&String, &JsonValue)> = object.iter().filter(|(key, _)| !NOT_STEPS.contains(&key.as_str())).collect();
    entries.sort_by_key(|(key, _)| key.as_str() != "table");
    entries.into_iter().flat_map(|(key, value)| nodes_for(key, value)).collect()
}

fn number(object: &Map<String, JsonValue>, key: &str) -> Option<f64> {
    object.get(key).and_then(JsonValue::as_f64)
}

fn text<'a>(object: &'a Map<String, JsonValue>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(JsonValue::as_str)
}

fn node_of(key: &str, object: &Map<String, JsonValue>) -> Node {
    let mut node = Node { label: label_of(key, object), children: children_of(object), ..Default::default() };
    node.cost = number(object, "cost");
    node.rows = number(object, "rows");
    // What ANALYZE measured. The tree carries it per loop, as MySQL's does (the view multiplies
    // by the loops); MariaDB gives the rows of a table per loop but times and output rows in total
    node.loops = number(object, "r_loops");
    let per_loop = |total: f64| total / node.loops.filter(|loops| *loops > 0.0).unwrap_or(1.0);
    node.actual_rows = number(object, "r_rows").or_else(|| number(object, "r_output_rows").map(per_loop));
    // A table's time is reading it plus checking its conditions
    let table_time = match (number(object, "r_table_time_ms"), number(object, "r_other_time_ms")) {
        (None, None) => None,
        (read, other) => Some(read.unwrap_or(0.0) + other.unwrap_or(0.0)),
    };
    node.actual_time_last = number(object, "r_total_time_ms").or(table_time).map(per_loop);
    node
}

fn label_of(key: &str, object: &Map<String, JsonValue>) -> String {
    match key {
        "query_block" => {
            let id = number(object, "select_id").map_or(String::new(), |id| format!(" #{id}"));
            match text(object, "operation") {
                Some(operation) => format!("{operation}{id}"),
                None => format!("Query block{id}"),
            }
        }
        "table" => {
            if let Some(message) = text(object, "message") {
                return message.to_string();
            }
            let mut label = format!("{} on {}", text(object, "access_type").unwrap_or("table"), text(object, "table_name").unwrap_or("?"));
            if let Some(index) = text(object, "key") {
                label += &format!(" using {index}");
            }
            if let Some(condition) = text(object, "attached_condition") {
                label += &format!(" where {condition}");
            }
            label
        }
        "filesort" => match text(object, "sort_key") {
            Some(sort_key) => format!("Filesort ({sort_key})"),
            None => "Filesort".into(),
        },
        "temporary_table" => "Temporary table".into(),
        "union_result" => "Union".into(),
        "block-nl-join" => format!("Block nested loop join{}", text(object, "join_type").map_or(String::new(), |t| format!(" ({t})"))),
        other => {
            let words = other.replace(['_', '-'], " ");
            let mut chars = words.chars();
            chars.next().map_or(String::new(), |first| first.to_uppercase().collect::<String>() + chars.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// `EXPLAIN FORMAT=JSON` of a join with GROUP BY and ORDER BY, from MariaDB 11.4.
    fn join_plan() -> JsonValue {
        json!({"query_block": {"select_id": 1, "cost": 0.01484632, "filesort": {"sort_key": "sum(o.total) desc", "temporary_table": {"nested_loop": [
            {"table": {"table_name": "o", "access_type": "ALL", "possible_keys": ["customer_id"], "loops": 1, "rows": 3, "cost": 0.0113438, "filtered": 100, "attached_condition": "o.customer_id is not null"}},
            {"table": {"table_name": "c", "access_type": "eq_ref", "possible_keys": ["PRIMARY"], "key": "PRIMARY", "key_length": "4", "used_key_parts": ["id"], "ref": ["probe.o.customer_id"], "loops": 3, "rows": 1, "cost": 0.00350252, "filtered": 100, "attached_condition": "c.age > 0"}}
        ]}}}})
    }

    #[test]
    fn a_plan_becomes_a_tree_of_steps() {
        let tree = tree_from_json(&join_plan()).unwrap();
        let root = &tree[0];
        assert_eq!(root["label"], "Query block #1");
        assert_eq!(root["cost"], json!(0.01484632));
        assert_eq!(root["actualRows"], json!(null));
        let sort = &root["children"][0];
        assert_eq!(sort["label"], "Filesort (sum(o.total) desc)");
        let temporary = &sort["children"][0];
        assert_eq!(temporary["label"], "Temporary table");
        let nested = &temporary["children"][0];
        assert_eq!(nested["label"], "Nested loop");
        let tables = nested["children"].as_array().unwrap();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0]["label"], "ALL on o where o.customer_id is not null");
        assert_eq!((tables[0]["rows"].clone(), tables[0]["cost"].clone()), (json!(3.0), json!(0.0113438)));
        assert_eq!(tables[1]["label"], "eq_ref on c using PRIMARY where c.age > 0");
        assert_eq!(tables[1]["children"], json!([]));
    }

    #[test]
    fn analyze_adds_what_was_measured() {
        let analyzed = json!({"query_optimization": {"r_total_time_ms": 0.02}, "query_block": {"select_id": 1, "r_loops": 1, "r_total_time_ms": 0.115, "table": {
            "table_name": "customers", "access_type": "ALL", "r_loops": 1, "rows": 3, "r_rows": 3, "r_table_time_ms": 0.01, "r_other_time_ms": 0.004,
            "r_engine_stats": {"pages_accessed": 1}, "filtered": 100, "r_filtered": 66.7, "attached_condition": "customers.age > 1"}}});
        let tree = tree_from_json(&analyzed).unwrap();
        let root = &tree[0];
        assert_eq!((root["loops"].clone(), root["actualTimeLast"].clone()), (json!(1.0), json!(0.115)));
        // The block takes the rows of the step on top
        assert_eq!(root["actualRows"], json!(3.0));
        let table = &root["children"][0];
        assert_eq!(table["label"], "ALL on customers where customers.age > 1");
        assert_eq!((table["actualRows"].clone(), table["loops"].clone(), table["actualTimeLast"].clone()), (json!(3.0), json!(1.0), json!(0.014)));
        // The bookkeeping isn't a step
        assert_eq!(table["children"], json!([]));
        assert_eq!(root["children"].as_array().unwrap().len(), 1);
    }

    /// The view multiplies a step's time and rows by its loops, as MySQL reports them per loop.
    /// MariaDB reports totals, so a step that ran three times must come as a third of them.
    #[test]
    fn measured_times_and_rows_come_per_loop() {
        let analyzed = json!({"query_block": {"select_id": 1, "r_loops": 1, "r_total_time_ms": 0.12, "filesort": {
            "sort_key": "x", "r_loops": 2, "r_total_time_ms": 0.02, "r_output_rows": 4, "nested_loop": [
                {"table": {"table_name": "o", "access_type": "ALL", "r_loops": 1, "rows": 3, "r_rows": 3, "r_table_time_ms": 0.03, "r_other_time_ms": 0.01}},
                {"table": {"table_name": "c", "access_type": "eq_ref", "r_loops": 3, "rows": 1, "r_rows": 1, "r_table_time_ms": 0.006, "r_other_time_ms": 0.054}}
            ]}}});
        let tree = tree_from_json(&analyzed).unwrap();
        let close = |value: &JsonValue, expected: f64| (value.as_f64().unwrap() - expected).abs() < 1e-9;
        let sort = &tree[0]["children"][0];
        assert!(close(&sort["actualTimeLast"], 0.01) && close(&sort["actualRows"], 2.0), "{sort}");
        let tables = sort["children"][0]["children"].as_array().unwrap();
        // The time of a table is reading it and checking its conditions
        assert!(close(&tables[0]["actualTimeLast"], 0.04), "{}", tables[0]);
        assert!(close(&tables[1]["actualTimeLast"], 0.02) && close(&tables[1]["actualRows"], 1.0), "{}", tables[1]);
        assert!(close(&tree[0]["actualTimeLast"], 0.12));
    }

    #[test]
    fn joins_unions_and_empty_plans_have_steps_too() {
        // MariaDB 10.6 joins with a join buffer instead of a nested loop
        let buffered = json!({"query_block": {"select_id": 1, "filesort": {"sort_key": "x", "temporary_table": {
            "table": {"table_name": "c", "access_type": "ALL", "rows": 3, "filtered": 100},
            "block-nl-join": {"table": {"table_name": "o", "access_type": "ALL", "rows": 3}, "buffer_type": "flat", "join_type": "BNL", "attached_condition": "o.customer_id = c.`id`"}}}}});
        let tree = tree_from_json(&buffered).unwrap();
        let temporary = &tree[0]["children"][0]["children"][0];
        let labels: Vec<&str> = temporary["children"].as_array().unwrap().iter().map(|n| n["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["ALL on c", "Block nested loop join (BNL)"]);
        assert_eq!(temporary["children"][1]["children"][0]["label"], "ALL on o");

        let union = json!({"query_block": {"union_result": {"table_name": "<union1,3>", "access_type": "ALL", "query_specifications": [
            {"query_block": {"select_id": 1, "table": {"table_name": "customers", "access_type": "ALL", "rows": 3}}},
            {"query_block": {"select_id": 3, "operation": "UNION", "table": {"table_name": "customers", "access_type": "ALL", "rows": 3}}}]}}});
        let tree = tree_from_json(&union).unwrap();
        let result = &tree[0]["children"][0];
        assert_eq!(result["label"], "Union");
        let parts: Vec<&str> = result["children"].as_array().unwrap().iter().map(|n| n["label"].as_str().unwrap()).collect();
        assert_eq!(parts, ["Query block #1", "UNION #3"]);

        let nothing = json!({"query_block": {"select_id": 1, "table": {"message": "No tables used"}}});
        assert_eq!(tree_from_json(&nothing).unwrap()[0]["children"][0]["label"], "No tables used");
        assert!(tree_from_json(&json!({"other": 1})).is_err());
    }
}
