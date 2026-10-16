//! Values as JSON for the frontend, whichever engine they come from.

use serde_json::Value as JsonValue;

/// Largest integer JavaScript represents exactly (2^53 - 1).
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// An integer as JSON: a number while JavaScript can hold it exactly, text beyond that, so
/// a big bigint (ids, snowflakes) keeps every digit when it's shown, edited or used as a key.
pub fn int_to_json(n: i64) -> JsonValue {
    if (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&n) {
        JsonValue::Number(n.into())
    } else {
        JsonValue::String(n.to_string())
    }
}
