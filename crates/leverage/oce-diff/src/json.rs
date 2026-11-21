//! The diff's JSON spelling: keys sorted at every depth, two-space indentation, one final line
//! feed. It is the spelling the `oce` CLI's reports use, so a diff embedded in a report and a
//! diff written alone are the same text.

use serde::Serialize;
use serde_json::Value;

/// Write `value` canonically. Serializing the diff's types (strings, integers, Booleans, lists
/// and closed objects) cannot fail; `null` is still well-formed JSON if it ever did.
pub(crate) fn canonical<T: Serialize>(value: &T) -> String {
    let mut value = serde_json::to_value(value).unwrap_or(Value::Null);
    sort_keys(&mut value);
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| String::from("null"));
    text.push('\n');
    text
}

/// Sort every object's keys. `serde_json`'s map is ordered by key unless another crate in the
/// build turns its `preserve_order` feature on; sorting here keeps the bytes the same either way.
fn sort_keys(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.sort_keys();
            for value in map.values_mut() {
                sort_keys(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                sort_keys(value);
            }
        }
        _ => {}
    }
}
