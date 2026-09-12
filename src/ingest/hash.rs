//! `InputHash`: joins a permission prompt to the call it belongs to.
//!
//! The forwarder hashes `tool_input` before anything crosses the socket, and
//! the transcript watcher hashes the `input` of a `tool_use` block. Both run
//! this function, so it only has to agree with itself.
//!
//! Canonical form is `serde_json`'s compact serialisation. Without the
//! `preserve_order` feature a `Value::Object` is a `BTreeMap`, so keys come
//! out sorted and two inputs that differ only in key order hash the same.

use serde_json::Value;

use crate::model::InputHash;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a over a byte slice.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(FNV_OFFSET, |h, &b| (h ^ u64::from(b)).wrapping_mul(FNV_PRIME))
}

/// Hash of the canonical JSON form of a tool input.
pub fn input_hash(input: &Value) -> InputHash {
    let canonical = input.to_string();
    InputHash(fnv1a(canonical.as_bytes()).to_be_bytes())
}

impl InputHash {
    /// Sixteen lowercase hex digits, as the wire carries it.
    pub fn to_hex(self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Inverse of `to_hex`. Exactly sixteen hex digits, either case.
    pub fn from_hex(s: &str) -> Option<InputHash> {
        if s.len() != 16 || !s.is_ascii() {
            return None;
        }
        let bytes: Vec<u8> = s
            .as_bytes()
            .chunks(2)
            .map(|pair| std::str::from_utf8(pair).ok())
            .map(|pair| pair.and_then(|p| u8::from_str_radix(p, 16).ok()))
            .collect::<Option<Vec<u8>>>()?;
        bytes.try_into().ok().map(InputHash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn should_hash_equal_when_only_key_order_differs() {
        let a: Value = serde_json::from_str(r#"{"command":"ls","description":"x"}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"description":"x","command":"ls"}"#).unwrap();
        assert_eq!(input_hash(&a), input_hash(&b));
    }

    #[test]
    fn should_hash_differently_when_a_value_differs() {
        assert_ne!(
            input_hash(&json!({"command": "ls"})),
            input_hash(&json!({"command": "ls -la"}))
        );
    }

    #[test]
    fn should_match_the_published_fnv1a_vectors() {
        assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
    }

    #[test]
    fn should_round_trip_through_hex_and_reject_bad_digits() {
        let h = input_hash(&json!({"a": 1}));
        assert_eq!(InputHash::from_hex(&h.to_hex()), Some(h));
        assert_eq!(InputHash::from_hex("zz00000000000000"), None);
        assert_eq!(InputHash::from_hex("00"), None);
        assert_eq!(InputHash::from_hex("ééééééé00"), None);
    }
}
