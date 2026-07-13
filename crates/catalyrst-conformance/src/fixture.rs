use serde::de::{Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fixture {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub request: RecordedRequest,
    pub response: RecordedResponse,
    pub captured_from: String,
    pub captured_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volatile_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "deserialize_query"
    )]
    pub query: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StringOrSeq {
    One(String),
    Many(Vec<String>),
}

impl From<StringOrSeq> for Vec<String> {
    fn from(v: StringOrSeq) -> Self {
        match v {
            StringOrSeq::One(s) => vec![s],
            StringOrSeq::Many(v) => v,
        }
    }
}

fn deserialize_query<'de, D>(deserializer: D) -> Result<BTreeMap<String, Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    struct QueryVisitor;

    impl<'de> Visitor<'de> for QueryVisitor {
        type Value = BTreeMap<String, Vec<String>>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a map of query params where each value is a string or an array of strings")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut out = BTreeMap::new();
            while let Some((key, value)) = access.next_entry::<String, StringOrSeq>()? {
                out.insert(key, value.into());
            }
            Ok(out)
        }
    }

    deserializer.deserialize_map(QueryVisitor)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedResponse {
    pub status: u16,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_bytes_b64: Option<String>,
}

impl RecordedResponse {
    #[allow(dead_code)]
    pub fn is_json(&self) -> bool {
        self.body_json.is_some()
    }
}

#[allow(dead_code)]
pub fn scrub_volatile_path(value: &mut serde_json::Value, path: &str) {
    const SENTINEL: &str = "<<VOLATILE>>";
    let parts: Vec<&str> = path.split('.').collect();
    scrub_recursive(value, &parts, SENTINEL);
}

#[allow(dead_code)]
fn scrub_recursive(value: &mut serde_json::Value, parts: &[&str], sentinel: &str) {
    if parts.is_empty() {
        *value = serde_json::Value::String(sentinel.to_string());
        return;
    }
    let (head, tail) = (parts[0], &parts[1..]);
    match value {
        serde_json::Value::Object(map) => {
            if let Some(child) = map.get_mut(head) {
                scrub_recursive(child, tail, sentinel);
            }
        }
        serde_json::Value::Array(arr) => {
            for child in arr.iter_mut() {
                scrub_recursive(child, parts, sentinel);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scrub_simple_path() {
        let mut v = json!({"a": {"b": "secret"}});
        scrub_volatile_path(&mut v, "a.b");
        assert_eq!(v, json!({"a": {"b": "<<VOLATILE>>"}}));
    }

    #[test]
    fn scrub_missing_path_is_noop() {
        let mut v = json!({"a": 1});
        scrub_volatile_path(&mut v, "x.y.z");
        assert_eq!(v, json!({"a": 1}));
    }

    #[test]
    fn scrub_walks_into_arrays() {
        let mut v = json!({"items": [{"url": "a"}, {"url": "b"}]});
        scrub_volatile_path(&mut v, "items.url");
        assert_eq!(
            v,
            json!({"items": [{"url": "<<VOLATILE>>"}, {"url": "<<VOLATILE>>"}]})
        );
    }
}
