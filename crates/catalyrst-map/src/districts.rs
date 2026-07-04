use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

const DISTRICTS_JSON: &str = include_str!("../data/districts.json");
const CONTRIBUTIONS_JSON: &str = include_str!("../data/contributions.json");

pub fn districts() -> &'static Vec<Value> {
    static D: OnceLock<Vec<Value>> = OnceLock::new();
    D.get_or_init(|| serde_json::from_str(DISTRICTS_JSON).expect("districts.json must parse"))
}

pub fn district(id: &str) -> Option<&'static Value> {
    districts()
        .iter()
        .find(|d| d.get("id").and_then(|v| v.as_str()) == Some(id))
}

pub fn contributions() -> &'static HashMap<String, Value> {
    static C: OnceLock<HashMap<String, Value>> = OnceLock::new();
    C.get_or_init(|| {
        serde_json::from_str(CONTRIBUTIONS_JSON).expect("contributions.json must parse")
    })
}

pub fn contributions_for(address: &str) -> Value {
    contributions()
        .get(&address.to_lowercase())
        .cloned()
        .unwrap_or_else(|| Value::Array(vec![]))
}
