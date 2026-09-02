use serde::Serialize;
use serde_json::Value;

use crate::error::Result;
use crate::identity::ContentHash;

/// Serialize a schema-defined value deterministically.
///
/// Struct field order is declaration order and all semantic maps in the public
/// model use `BTreeMap`. This function additionally recursively sorts generic
/// JSON object keys so provider payloads cannot depend on insertion order.
pub fn to_canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let mut json = serde_json::to_value(value)?;
    sort_json(&mut json);
    Ok(serde_json::to_vec(&json)?)
}

pub fn to_canonical_json_pretty<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    canonical_value_pretty(serde_json::to_value(value)?)
}

pub(crate) fn canonical_value_pretty(mut json: Value) -> Result<Vec<u8>> {
    sort_json(&mut json);
    let mut encoded = serde_json::to_vec_pretty(&json)?;
    encoded.push(b'\n');
    Ok(encoded)
}

pub fn canonical_hash<T: Serialize + ?Sized>(domain: &str, value: &T) -> Result<ContentHash> {
    let bytes = to_canonical_json(value)?;
    Ok(ContentHash::from_domain_bytes(domain, &bytes))
}

fn sort_json(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                sort_json(item);
            }
        }
        Value::Object(map) => {
            let old = std::mem::take(map);
            let mut entries: Vec<_> = old.into_iter().collect();
            entries.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
            for (key, mut item) in entries {
                sort_json(&mut item);
                map.insert(key, item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn sorts_generic_json_keys() {
        let mut map = HashMap::new();
        map.insert("z", 1);
        map.insert("a", 2);
        let encoded = to_canonical_json(&map).expect("canonical JSON");
        assert_eq!(encoded, br#"{"a":2,"z":1}"#);
    }
}
