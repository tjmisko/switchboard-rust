//! Reading typed values out of `serde_json::Value`.
//!
//! Every function here returns a reference into the `Value` it was given, or
//! a `Copy` scalar, so nothing is cloned until a caller decides to own it.
//! The lifetime `'a` on the input is the whole contract: the `&str` you get
//! back cannot outlive the JSON it was read from.

use serde_json::Value;

/// `v[key]` as a string. `None` when absent or not a string.
pub fn str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// `v[key]` as a string with at least one non-whitespace character.
pub fn nonblank_str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    str_at(v, key).filter(|s| !s.trim().is_empty())
}

/// `v[key]` as a non-negative integer.
pub fn u64_at(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_u64)
}

/// `v[key]` as a boolean.
pub fn bool_at(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(Value::as_bool)
}

/// Follow a path of object keys: `at(v, &["origin", "kind"])`.
///
/// Written as a fold over `Option<&Value>`: each step either descends or
/// stays `None` for the rest of the walk.
pub fn at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(v, |cur, key| cur.get(key))
}

/// `at(v, path)` as a string.
pub fn str_path<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    at(v, path).and_then(Value::as_str)
}

/// The elements of `v[key]` when it is an array, and nothing otherwise.
///
/// `Option<&Vec<Value>>` is itself iterable (zero or one item), so
/// `flatten` turns "maybe an array" into "the array's elements, or none"
/// without a branch.
pub fn array_at<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    v.get(key).and_then(Value::as_array).into_iter().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn should_stop_walking_when_a_path_segment_is_missing() {
        let v = json!({"origin": {"kind": "human"}});
        assert_eq!(str_path(&v, &["origin", "kind"]), Some("human"));
        assert_eq!(str_path(&v, &["origin", "nope", "kind"]), None);
        assert_eq!(at(&v, &[]), Some(&v));
    }

    #[test]
    fn should_yield_nothing_when_the_key_is_not_an_array() {
        let v = json!({"content": "a string"});
        assert_eq!(array_at(&v, "content").count(), 0);
    }
}
