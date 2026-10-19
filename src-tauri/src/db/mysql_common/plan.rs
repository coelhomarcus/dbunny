//! Query plans: turns `EXPLAIN FORMAT=TREE` / `EXPLAIN ANALYZE` text into nodes, and tells
//! which statements may be analyzed.

use serde_json::Value as JsonValue;

use crate::db::mysql_common::split::{next_token, Kind};

/// What one line of the plan says about its node.
#[derive(Debug, Default)]
pub(crate) struct Node {
    pub(crate) label: String,
    pub(crate) cost: Option<f64>,
    pub(crate) rows: Option<f64>,
    pub(crate) actual_time_first: Option<f64>,
    pub(crate) actual_time_last: Option<f64>,
    pub(crate) actual_rows: Option<f64>,
    pub(crate) loops: Option<f64>,
    pub(crate) children: Vec<Node>,
}

impl Node {
    pub(crate) fn to_json(&self) -> JsonValue {
        let number = |value: Option<f64>| value.and_then(serde_json::Number::from_f64).map_or(JsonValue::Null, JsonValue::Number);
        serde_json::json!({
            "label": self.label,
            "cost": number(self.cost),
            "rows": number(self.rows),
            "actualTimeFirst": number(self.actual_time_first),
            "actualTimeLast": number(self.actual_time_last),
            "actualRows": number(self.actual_rows),
            "loops": number(self.loops),
            "children": self.children.iter().map(Node::to_json).collect::<Vec<_>>(),
        })
    }
}

/// How many spaces a level of the tree is indented by.
const INDENT: usize = 4;

/// The nodes of a plan as `[{ label, cost, rows, actualTimeFirst, actualTimeLast, actualRows,
/// loops, children }]`; what the plan doesn't say is null.
///
/// Every node is a line `-> label  (cost=C rows=R) (actual time=F..L rows=N loops=K)`, indented
/// four spaces under its parent. A line without an arrow continues the label before it.
pub(crate) fn parse_tree(text: &str) -> Result<JsonValue, String> {
    // (depth, the node's text)
    let mut lines: Vec<(usize, String)> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("->") {
            lines.push(((line.len() - trimmed.len()) / INDENT, rest.trim().to_string()));
        } else if !trimmed.is_empty() {
            if let Some((_, previous)) = lines.last_mut() {
                previous.push(' ');
                previous.push_str(trimmed.trim_end());
            }
        }
    }
    if lines.is_empty() {
        return Err("The server returned no plan".into());
    }

    // Each node waits on the stack of its ancestors until a line at its depth or above arrives
    let mut roots: Vec<Node> = Vec::new();
    let mut open: Vec<(usize, Node)> = Vec::new();
    let close_one = |open: &mut Vec<(usize, Node)>, roots: &mut Vec<Node>| {
        let (_, node) = open.pop().expect("something is open");
        match open.last_mut() {
            Some((_, parent)) => parent.children.push(node),
            None => roots.push(node),
        }
    };
    for (depth, raw) in lines {
        while open.last().is_some_and(|(open_depth, _)| *open_depth >= depth) {
            close_one(&mut open, &mut roots);
        }
        open.push((depth, parse_node(&raw)));
    }
    while !open.is_empty() {
        close_one(&mut open, &mut roots);
    }
    Ok(JsonValue::Array(roots.iter().map(Node::to_json).collect()))
}

/// A node from the text after its arrow.
fn parse_node(raw: &str) -> Node {
    let mut text = raw.to_string();
    let mut node = Node::default();

    if let Some(stripped) = text.trim_end().strip_suffix("(never executed)") {
        text = stripped.to_string();
    } else if let Some(group) = take_group(&mut text, "(actual time=") {
        // `time=F..L rows=N loops=K`, after the "actual " already cut off by the marker
        for (key, value) in group.split_whitespace().filter_map(|part| part.split_once('=')) {
            match key {
                "time" => {
                    let (first, last) = value.split_once("..").unwrap_or((value, value));
                    node.actual_time_first = first.parse().ok();
                    node.actual_time_last = last.parse().ok();
                }
                "rows" => node.actual_rows = value.parse().ok(),
                "loops" => node.loops = value.parse().ok(),
                _ => {}
            }
        }
    }
    if let Some(group) = take_group(&mut text, "(cost=") {
        for (key, value) in group.split_whitespace().filter_map(|part| part.split_once('=')) {
            match key {
                "cost" => node.cost = value.parse().ok(),
                "rows" => node.rows = value.parse().ok(),
                _ => {}
            }
        }
    }
    node.label = text.trim().to_string();
    node
}

/// Cuts the last parenthesized group that starts with `marker` off the end of `text` and
/// returns what is inside after the marker's own name. `(cost=1 rows=2)` gives `cost=1 rows=2`,
/// `(actual time=1..2 rows=3 loops=1)` gives `time=1..2 rows=3 loops=1`.
fn take_group(text: &mut String, marker: &str) -> Option<String> {
    let start = text.rfind(marker)?;
    let body = text[start..].trim_end().strip_suffix(')')?;
    let group = body[1..].strip_prefix("actual ").unwrap_or(&body[1..]).to_string();
    text.truncate(start);
    Some(group)
}

/// Whether `sql` only reads, so that `EXPLAIN ANALYZE`, which runs it, changes nothing.
///
/// A query starts with SELECT, TABLE, VALUES or a parenthesis. WITH starts either a query or
/// a statement that writes (`WITH … DELETE`): the verb is the first word outside parentheses
/// that is neither a CTE's name nor `AS`.
pub(crate) fn is_reading_statement(sql: &str) -> bool {
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    let mut first = true;
    let mut with = false;
    let mut depth = 0usize;
    while let Some(token) = next_token(&chars, i) {
        i = token.end;
        match &token.kind {
            Kind::Punct('(') => {
                if first {
                    return true;
                }
                depth += 1;
            }
            Kind::Punct(')') => depth = depth.saturating_sub(1),
            Kind::Word(word) if first => match word.as_str() {
                "SELECT" | "TABLE" | "VALUES" => return true,
                "WITH" => with = true,
                _ => return false,
            },
            Kind::Word(word) if with && depth == 0 => match word.as_str() {
                "SELECT" | "TABLE" | "VALUES" => return true,
                "UPDATE" | "DELETE" | "INSERT" | "REPLACE" => return false,
                _ => {}
            },
            _ if first => return false,
            _ => {}
        }
        first = false;
    }
    false
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const TREE: &str = "-> Sort: b.`name`\n    -> Stream results  (cost=1 rows=0.667)\n        -> Inner hash join (a.b_id = b.id)  (cost=1 rows=0.667)\n            -> Filter: (a.v <> 'z')  (cost=0.225 rows=1)\n                -> Table scan on a  (cost=0.225 rows=3)\n            -> Hash\n                -> Table scan on b  (cost=0.45 rows=2)\n";

    const ANALYZED: &str = "-> Sort: b.`name`  (actual time=0.37..0.37 rows=3 loops=1)\n    -> Stream results  (cost=1 rows=0.667) (actual time=0.349..0.351 rows=3 loops=1)\n        -> Inner hash join (a.b_id = b.id)  (cost=1 rows=0.667) (actual time=0.335..0.337 rows=3 loops=1)\n            -> Filter: (a.v <> 'z')  (cost=0.225 rows=1) (actual time=0.00387..0.00487 rows=3 loops=1)\n                -> Table scan on a  (cost=0.225 rows=3) (actual time=0.00258..0.00325 rows=3 loops=1)\n            -> Hash\n                -> Table scan on b  (cost=0.45 rows=2) (actual time=0.306..0.307 rows=2 loops=1)\n";

    #[test]
    fn the_tree_becomes_nodes_with_costs() {
        let plan = parse_tree(TREE).unwrap();
        let roots = plan.as_array().unwrap();
        assert_eq!(roots.len(), 1);
        let sort = &roots[0];
        assert_eq!(sort["label"], "Sort: b.`name`");
        assert_eq!(sort["cost"], json!(null));
        assert_eq!(sort["children"].as_array().unwrap().len(), 1);

        let stream = &sort["children"][0];
        assert_eq!(stream["label"], "Stream results");
        assert_eq!(stream["cost"], json!(1.0));
        assert_eq!(stream["rows"], json!(0.667));

        let join = &stream["children"][0];
        assert_eq!(join["label"], "Inner hash join (a.b_id = b.id)");
        assert_eq!(join["children"].as_array().unwrap().len(), 2);
        assert_eq!(join["actualRows"], json!(null));
        assert_eq!(join["actualTimeFirst"], json!(null));
        assert_eq!(join["loops"], json!(null));

        let filter = &join["children"][0];
        assert_eq!(filter["label"], "Filter: (a.v <> 'z')");
        let scan_a = &filter["children"][0];
        assert_eq!((scan_a["label"].as_str(), scan_a["cost"].clone(), scan_a["rows"].clone()), (Some("Table scan on a"), json!(0.225), json!(3.0)));

        let hash = &join["children"][1];
        assert_eq!(hash["label"], "Hash");
        assert_eq!(hash["cost"], json!(null));
        assert_eq!(hash["children"][0]["label"], "Table scan on b");
    }

    #[test]
    fn analyze_adds_actual_times_rows_and_loops() {
        let plan = parse_tree(ANALYZED).unwrap();
        let scan_a = &plan[0]["children"][0]["children"][0]["children"][0]["children"][0];
        assert_eq!(scan_a["label"], "Table scan on a");
        assert_eq!(scan_a["cost"], json!(0.225));
        assert_eq!(scan_a["actualTimeFirst"], json!(0.00258));
        assert_eq!(scan_a["actualTimeLast"], json!(0.00325));
        assert_eq!(scan_a["actualRows"], json!(3.0));
        assert_eq!(scan_a["loops"], json!(1.0));
        // A node that has actual figures but no cost
        assert_eq!(plan[0]["label"], "Sort: b.`name`");
        assert_eq!(plan[0]["actualRows"], json!(3.0));
        assert_eq!(plan[0]["cost"], json!(null));
    }

    #[test]
    fn a_node_that_never_ran_has_no_actual_figures() {
        let plan = parse_tree("-> Limit: 1 row(s)  (cost=2 rows=1) (actual time=0.1..0.1 rows=1 loops=1)\n    -> Table scan on t  (cost=2 rows=1) (never executed)\n").unwrap();
        let scan = &plan[0]["children"][0];
        assert_eq!(scan["label"], "Table scan on t");
        assert_eq!(scan["cost"], json!(2.0));
        assert_eq!(scan["actualRows"], json!(null));
        assert_eq!(scan["loops"], json!(null));
    }

    #[test]
    fn a_line_without_an_arrow_continues_the_label_before_it() {
        let plan = parse_tree("-> Filter: (a > 1)\n   and (b < 2)  (cost=1 rows=2)\n    -> Table scan on t  (cost=1 rows=2)\n").unwrap();
        assert_eq!(plan[0]["label"], "Filter: (a > 1) and (b < 2)");
        assert_eq!(plan[0]["rows"], json!(2.0));
        assert_eq!(plan[0]["children"][0]["label"], "Table scan on t");
    }

    #[test]
    fn several_roots_and_exponent_numbers_are_read() {
        let plan = parse_tree("-> Table scan on t  (cost=1.5e+06 rows=1e+06)\n-> Select #2 (subquery in condition; run only once)\n").unwrap();
        assert_eq!(plan.as_array().unwrap().len(), 2);
        assert_eq!(plan[0]["cost"], json!(1_500_000.0));
        assert_eq!(plan[0]["rows"], json!(1_000_000.0));
    }

    #[test]
    fn text_with_no_node_is_an_error() {
        assert!(parse_tree("").is_err());
        assert!(parse_tree("   \n").is_err());
    }

    #[test]
    fn only_reading_statements_may_be_analyzed() {
        for sql in [
            "SELECT 1",
            "  -- note\n select * from t",
            "# c\nTABLE t",
            "(SELECT 1) UNION (SELECT 2)",
            "WITH x AS (SELECT 1) SELECT * FROM x",
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 3) SELECT * FROM n",
            "SELECT 1 /* UPDATE t SET a = 1 */",
            "select * from `update`",
            "VALUES ROW(1), ROW(2)",
        ] {
            assert!(is_reading_statement(sql), "{sql} reads");
        }
        for sql in [
            "UPDATE t SET a = 1",
            "DELETE FROM t",
            "INSERT INTO t VALUES (1)",
            "WITH x AS (SELECT 1) DELETE FROM t WHERE a IN (SELECT * FROM x)",
            "WITH x AS (SELECT 1) UPDATE t SET a = 1",
            "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x",
            "CREATE TABLE z (a INT)",
            "EXPLAIN SELECT 1",
            "",
        ] {
            assert!(!is_reading_statement(sql), "{sql} writes or isn't a query");
        }
    }
}
