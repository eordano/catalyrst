//! Ingest-side telemetry contract guard.
//!
//! The authoritative enforcement layer for telemetry event shapes. Mirrors the
//! sites TS validator (`packages/core/src/lib/telemetry/validate.ts`) against the
//! machine-readable contract generated from the sites TS registry
//! (`telemetry-contract.json`).
//!
//! Loaded fail-open: no `TELEMETRY_CONTRACT_PATH` / missing / unparseable file
//! disables validation (accept everything). When enabled, a bad event is flagged
//! (quarantine-by-`invalid_reason`), never rejected -- no data loss, no client
//! breakage.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

/// One declared property of a contract event (or a context prop).
#[derive(Debug, Clone, Deserialize)]
pub struct ContractProp {
    /// `string | number | boolean | enum-string | enum-number | unknown`.
    pub kind: String,
    /// Permitted values for the `enum-*` kinds.
    #[serde(default)]
    pub values: Option<Vec<Value>>,
    #[serde(default)]
    pub optional: bool,
}

/// One event's declared shape.
#[derive(Debug, Clone, Deserialize)]
pub struct ContractEvent {
    /// A loose event opts out of prop validation entirely.
    #[serde(default)]
    pub loose: bool,
    #[serde(default)]
    pub props: HashMap<String, ContractProp>,
}

/// The whole machine-readable contract.
#[derive(Debug, Clone, Deserialize)]
pub struct Contract {
    #[serde(default)]
    pub version: u32,
    /// Injected context fields (story/variant/exp_key). Always allowed on any
    /// event as extra props, so this is informational only for validation.
    #[serde(default, rename = "contextProps")]
    pub context_props: HashMap<String, ContractProp>,
    #[serde(default)]
    pub events: HashMap<String, ContractEvent>,
}

/// Load the contract from `TELEMETRY_CONTRACT_PATH`. Fail-open: any of unset /
/// missing / unreadable / unparseable logs a single warning and returns `None`
/// (validation disabled -- accept every event as today).
pub fn load_from_env() -> Option<Arc<Contract>> {
    let path = match std::env::var("TELEMETRY_CONTRACT_PATH") {
        Ok(p) if !p.trim().is_empty() => p,
        _ => {
            tracing::warn!(
                "TELEMETRY_CONTRACT_PATH unset \u{2014} telemetry contract validation DISABLED (accepting all events)"
            );
            return None;
        }
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Contract>(&text) {
            Ok(contract) => {
                tracing::info!(
                    path = %path,
                    events = contract.events.len(),
                    version = contract.version,
                    "telemetry contract loaded \u{2014} validation ENABLED"
                );
                Some(Arc::new(contract))
            }
            Err(err) => {
                tracing::warn!(
                    path = %path,
                    error = %err,
                    "telemetry contract unparseable \u{2014} validation DISABLED (accepting all events)"
                );
                None
            }
        },
        Err(err) => {
            tracing::warn!(
                path = %path,
                error = %err,
                "telemetry contract file missing/unreadable \u{2014} validation DISABLED (accepting all events)"
            );
            None
        }
    }
}

/// JS-`typeof`-flavoured kind of a JSON value. Arrays and objects both read as
/// `"object"`, matching the TS `kindOf`.
fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
        Value::Array(_) | Value::Object(_) => "object",
    }
}

/// Numeric equality across JSON int/float representations (`5` == `5.0`),
/// matching JS where every number is an f64.
fn numbers_eq(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// Render a contract enum value bare (strings without quotes) for the `{a, b}`
/// list in a problem message, matching the TS `values.join(", ")`.
fn bare(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn join_values(values: &[Value]) -> String {
    values.iter().map(bare).collect::<Vec<_>>().join(", ")
}

/// Validate one DCL event's props against the contract. Returns a human-readable
/// problem string, or `None` when the event is valid.
///
/// Mirrors `validateEventAgainst` in sites `validate.ts`:
/// - unknown event -> problem
/// - loose event -> ok
/// - missing required prop -> problem
/// - wrong `string`/`number`/`boolean` kind -> problem
/// - `enum-string` / `enum-number` value not in `values` -> problem
/// - `unknown` kind -> accept
/// - extra props (incl. injected context fields) -> allowed
///
/// All problems found are collected and joined with `; ` (the TS collects an
/// array); collapsed here to a single `Option<String>` for the stored flag.
pub fn validate_event(contract: &Contract, event_name: &str, properties: &Value) -> Option<String> {
    let ev = match contract.events.get(event_name) {
        None => {
            return Some(format!(
                "unknown event \"{event_name}\" (not in the telemetry contract)"
            ))
        }
        Some(ev) => ev,
    };
    if ev.loose {
        return None;
    }

    // `properties` may legitimately be null / absent / not an object; treat any
    // non-object as "no props present".
    let empty = serde_json::Map::new();
    let props = properties.as_object().unwrap_or(&empty);

    let mut problems: Vec<String> = Vec::new();
    for (name, spec) in &ev.props {
        let val = match props.get(name) {
            Some(v) if !v.is_null() => v,
            _ => {
                if !spec.optional {
                    problems.push(format!("missing required prop \"{name}\""));
                }
                continue;
            }
        };
        let actual = kind_of(val);
        match spec.kind.as_str() {
            "string" | "number" | "boolean" if actual != spec.kind => {
                problems.push(format!(
                    "prop \"{name}\" should be {}, got {actual}",
                    spec.kind
                ));
            }
            "enum-string" => {
                if actual != "string" {
                    problems.push(format!("prop \"{name}\" should be a string, got {actual}"));
                } else if let Some(values) = &spec.values {
                    if !values.iter().any(|v| v == val) {
                        problems.push(format!(
                            "prop \"{name}\" = {val} is not one of {{{}}}",
                            join_values(values)
                        ));
                    }
                }
            }
            "enum-number" => {
                if actual != "number" {
                    problems.push(format!("prop \"{name}\" should be a number, got {actual}"));
                } else if let Some(values) = &spec.values {
                    if !values.iter().any(|v| numbers_eq(v, val)) {
                        problems.push(format!(
                            "prop \"{name}\" = {val} is not one of {{{}}}",
                            join_values(values)
                        ));
                    }
                }
            }
            // "unknown" (complex/object types) -- accept; the contract can't model it.
            _ => {}
        }
    }

    if problems.is_empty() {
        None
    } else {
        Some(problems.join("; "))
    }
}

#[cfg(test)]
mod validate_tests {
    use super::*;

    fn fixture() -> Contract {
        serde_json::from_str(
            r#"{
              "version": 1,
              "contextProps": {
                "story":   { "kind": "string", "optional": true },
                "variant": { "kind": "string", "optional": true },
                "exp_key": { "kind": "string", "optional": true }
              },
              "events": {
                "thing_clicked": {
                  "loose": false,
                  "props": {
                    "label":   { "kind": "string",       "optional": false },
                    "count":   { "kind": "number",       "optional": false },
                    "enabled": { "kind": "boolean",      "optional": false },
                    "mode":    { "kind": "enum-string",  "values": ["a", "b"], "optional": false },
                    "level":   { "kind": "enum-number",  "values": [1, 2, 3],  "optional": false },
                    "payload": { "kind": "unknown",      "optional": false },
                    "note":    { "kind": "string",       "optional": true }
                  }
                },
                "freeform": { "loose": true, "props": {} }
              }
            }"#,
        )
        .expect("fixture contract parses")
    }

    fn valid_props() -> Value {
        serde_json::json!({
            "label": "x",
            "count": 3,
            "enabled": true,
            "mode": "a",
            "level": 2,
            "payload": { "nested": 1 }
        })
    }

    #[test]
    fn validate_accepts_a_valid_event() {
        assert_eq!(
            validate_event(&fixture(), "thing_clicked", &valid_props()),
            None
        );
    }

    #[test]
    fn validate_rejects_an_unknown_event() {
        let problem = validate_event(&fixture(), "never_declared", &valid_props());
        assert!(
            problem.as_deref().unwrap_or("").contains("unknown event"),
            "got {problem:?}"
        );
    }

    #[test]
    fn validate_flags_a_missing_required_prop() {
        let mut props = valid_props();
        props.as_object_mut().unwrap().remove("label");
        let problem = validate_event(&fixture(), "thing_clicked", &props);
        assert!(
            problem
                .as_deref()
                .unwrap_or("")
                .contains("missing required prop \"label\""),
            "got {problem:?}"
        );
    }

    #[test]
    fn validate_flags_a_wrong_kind() {
        let mut props = valid_props();
        props["count"] = Value::String("3".into()); // number declared, string given
        let problem = validate_event(&fixture(), "thing_clicked", &props);
        assert!(
            problem
                .as_deref()
                .unwrap_or("")
                .contains("should be number"),
            "got {problem:?}"
        );
    }

    #[test]
    fn validate_flags_an_enum_string_out_of_set() {
        let mut props = valid_props();
        props["mode"] = Value::String("z".into());
        let problem = validate_event(&fixture(), "thing_clicked", &props);
        assert!(
            problem.as_deref().unwrap_or("").contains("is not one of"),
            "got {problem:?}"
        );
    }

    #[test]
    fn validate_flags_an_enum_number_out_of_set() {
        let mut props = valid_props();
        props["level"] = serde_json::json!(9);
        let problem = validate_event(&fixture(), "thing_clicked", &props);
        assert!(
            problem.as_deref().unwrap_or("").contains("is not one of"),
            "got {problem:?}"
        );
    }

    #[test]
    fn validate_loose_event_accepts_anything() {
        let props = serde_json::json!({ "whatever": 123, "shape": ["not", "declared"] });
        assert_eq!(validate_event(&fixture(), "freeform", &props), None);
    }

    #[test]
    fn validate_unknown_kind_accepts_any_shape() {
        let mut props = valid_props();
        props["payload"] = Value::String("now a string".into());
        assert_eq!(
            validate_event(&fixture(), "thing_clicked", &props),
            None,
            "the unknown kind must accept any value"
        );
    }

    #[test]
    fn validate_allows_extra_and_context_props() {
        let mut props = valid_props();
        let obj = props.as_object_mut().unwrap();
        obj.insert("story".into(), Value::String("checkout".into()));
        obj.insert("variant".into(), Value::String("b".into()));
        obj.insert("exp_key".into(), Value::String("exp-1".into()));
        obj.insert("some_future_prop".into(), serde_json::json!({ "x": 1 }));
        assert_eq!(validate_event(&fixture(), "thing_clicked", &props), None);
    }

    #[test]
    fn validate_optional_prop_may_be_absent() {
        // "note" is optional and omitted in valid_props; still valid.
        assert_eq!(
            validate_event(&fixture(), "thing_clicked", &valid_props()),
            None
        );
    }

    #[test]
    fn validate_number_int_float_enum_equivalence() {
        let mut props = valid_props();
        props["level"] = serde_json::json!(2.0); // 2.0 must match declared 2
        assert_eq!(validate_event(&fixture(), "thing_clicked", &props), None);
    }
}
