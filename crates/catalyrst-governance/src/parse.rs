use chrono::{DateTime, TimeZone, Utc};
use serde_json::{json, Value};

pub fn parse_ts(v: &Value) -> Option<DateTime<Utc>> {
    match v {
        Value::Null => None,
        Value::Number(n) => {
            let f = n.as_f64()?;
            let dt = if f > 1e12 {
                Utc.timestamp_millis_opt(f as i64).single()?
            } else {
                Utc.timestamp_opt(f as i64, 0).single()?
            };
            Some(dt)
        }
        Value::String(s) => parse_ts_str(s),
        _ => None,
    }
}

pub fn parse_ts_str(s: &str) -> Option<DateTime<Utc>> {
    if s.is_empty() {
        return None;
    }

    if s.starts_with('+') {
        return None;
    }
    let normalized = if let Some(stripped) = s.strip_suffix('Z') {
        format!("{stripped}+00:00")
    } else {
        s.to_string()
    };
    DateTime::parse_from_rfc3339(&normalized)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

pub fn field<'a>(obj: &'a Value, key: &str) -> &'a Value {
    obj.get(key).unwrap_or(&Value::Null)
}

pub fn opt_str(obj: &Value, key: &str) -> Option<String> {
    match obj.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

pub fn opt_i32(obj: &Value, key: &str) -> Option<i32> {
    obj.get(key).and_then(|v| v.as_i64()).map(|n| n as i32)
}

pub fn opt_i64(obj: &Value, key: &str) -> Option<i64> {
    obj.get(key).and_then(Value::as_i64)
}

pub fn opt_bool(obj: &Value, key: &str) -> Option<bool> {
    obj.get(key).and_then(Value::as_bool)
}

pub fn opt_json(obj: &Value, key: &str) -> Option<Value> {
    match obj.get(key) {
        Some(Value::Null) | None => None,
        Some(v) => Some(v.clone()),
    }
}

pub fn parse_page(resp: &Value) -> (Vec<Value>, u64) {
    let data = resp
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = resp.get("total").and_then(Value::as_u64).unwrap_or(0);
    (data, total)
}

pub fn parse_data_array(resp: &Value) -> Vec<Value> {
    resp.get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub fn parse_project_updates(resp: &Value) -> Vec<Value> {
    let data = field(resp, "data");
    let mut out = Vec::new();
    if let Some(arr) = field(data, "publicUpdates").as_array() {
        out.extend(arr.iter().cloned());
    }
    if let Some(arr) = field(data, "pendingUpdates").as_array() {
        out.extend(arr.iter().cloned());
    }
    out
}

pub fn parse_list_or_data(resp: &Value) -> Vec<Value> {
    match resp {
        Value::Array(arr) => arr.clone(),
        Value::Object(_) => match resp.get("data") {
            Some(Value::Array(arr)) => arr.clone(),
            Some(Value::Object(_)) => vec![resp.get("data").unwrap().clone()],
            _ => vec![resp.clone()],
        },
        _ => Vec::new(),
    }
}

pub fn parse_members(resp: &Value) -> Vec<String> {
    resp.get("data")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub fn project_is_active(project: &Value) -> bool {
    !matches!(
        project.get("status").and_then(Value::as_str),
        Some("finished") | Some("revoked")
    )
}

pub fn build_project_detail(project_raw: &Value, cfg: Option<&Value>, updates: &[Value]) -> Value {
    let mut out = project_raw.clone();
    let Some(obj) = out.as_object_mut() else {
        return out;
    };

    let about = cfg
        .and_then(|c| opt_str(c, "abstract"))
        .filter(|s| !s.trim().is_empty())
        .or_else(|| cfg.and_then(|c| opt_str(c, "description")))
        .unwrap_or_default();
    obj.insert("about".to_string(), Value::String(about));

    obj.insert("links".to_string(), Value::Array(Vec::new()));

    obj.insert(
        "personnel".to_string(),
        Value::Array(build_personnel(project_raw, cfg)),
    );
    obj.insert(
        "milestones".to_string(),
        Value::Array(build_milestones(cfg)),
    );
    obj.insert("updates".to_string(), Value::Array(updates.to_vec()));

    out
}

fn build_personnel(project_raw: &Value, cfg: Option<&Value>) -> Vec<Value> {
    let text = cfg
        .and_then(|c| opt_str(c, "personnel"))
        .unwrap_or_default();
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let address = cfg
        .and_then(|c| opt_str(c, "beneficiary"))
        .or_else(|| opt_str(project_raw, "author"));
    vec![json!({
        "id": "team",
        "name": "Team",
        "address": address,
        "role": "Team & Personnel",
        "about": text,
    })]
}

fn build_milestones(cfg: Option<&Value>) -> Vec<Value> {
    let arr = cfg
        .and_then(|c| c.get("milestones"))
        .and_then(Value::as_array)
        .or_else(|| cfg.and_then(|c| c.get("roadmap")).and_then(Value::as_array));
    let Some(arr) = arr else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let title = opt_str(m, "title")?;
            let description = opt_str(m, "tasks")
                .or_else(|| opt_str(m, "description"))
                .unwrap_or_default();
            Some(json!({
                "id": format!("m{}", i + 1),
                "title": title,
                "description": description,
                "delivery_date": opt_str(m, "delivery_date"),
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_ts_iso_with_z() {
        let dt = parse_ts(&json!("2024-01-02T03:04:05Z")).unwrap();
        assert_eq!(dt.to_rfc3339(), "2024-01-02T03:04:05+00:00");
    }

    #[test]
    fn parse_ts_iso_with_offset() {
        let dt = parse_ts(&json!("2024-01-02T03:04:05+00:00")).unwrap();
        assert_eq!(dt.timestamp(), 1704164645);
    }

    #[test]
    fn parse_ts_epoch_millis() {
        let dt = parse_ts(&json!(1_700_000_000_000i64)).unwrap();
        assert_eq!(dt.timestamp(), 1_700_000_000);
    }

    #[test]
    fn parse_ts_epoch_seconds() {
        let dt = parse_ts(&json!(1_700_000_000i64)).unwrap();
        assert_eq!(dt.timestamp(), 1_700_000_000);
    }

    #[test]
    fn parse_ts_far_future_plus_is_none() {
        assert!(parse_ts(&json!("+275760-09-13T00:00:00Z")).is_none());
    }

    #[test]
    fn parse_ts_null_and_empty_are_none() {
        assert!(parse_ts(&json!(null)).is_none());
        assert!(parse_ts(&json!("")).is_none());
        assert!(parse_ts(&json!("not-a-date")).is_none());
    }

    #[test]
    fn opt_extractors() {
        let o = json!({
            "title": "Hello",
            "discourse_id": 42,
            "total": 9_000_000_000i64,
            "enacted": true,
            "configuration": {"k": "v"},
            "empty": null
        });
        assert_eq!(opt_str(&o, "title").as_deref(), Some("Hello"));
        assert_eq!(opt_str(&o, "missing"), None);
        assert_eq!(opt_i32(&o, "discourse_id"), Some(42));
        assert_eq!(opt_i64(&o, "total"), Some(9_000_000_000));
        assert_eq!(opt_bool(&o, "enacted"), Some(true));
        assert_eq!(opt_json(&o, "configuration"), Some(json!({"k": "v"})));
        assert_eq!(opt_json(&o, "empty"), None);
        assert_eq!(opt_json(&o, "missing"), None);
    }

    #[test]
    fn parse_page_envelope() {
        let resp = json!({"data": [{"id": "a"}, {"id": "b"}], "total": 250});
        let (data, total) = parse_page(&resp);
        assert_eq!(data.len(), 2);
        assert_eq!(total, 250);

        let (empty, zero) = parse_page(&json!({}));
        assert!(empty.is_empty());
        assert_eq!(zero, 0);
    }

    #[test]
    fn project_updates_concat() {
        let resp = json!({
            "data": {
                "publicUpdates": [{"id": "u1"}, {"id": "u2"}],
                "pendingUpdates": [{"id": "u3"}]
            }
        });
        let ups = parse_project_updates(&resp);
        assert_eq!(ups.len(), 3);
        assert_eq!(ups[2]["id"], json!("u3"));

        assert!(parse_project_updates(&json!({"data": {}})).is_empty());
    }

    #[test]
    fn list_or_data_shapes() {
        assert_eq!(parse_list_or_data(&json!([{"id": "a"}])).len(), 1);
        assert_eq!(parse_list_or_data(&json!({"data": [{"id": "a"}]})).len(), 1);

        assert_eq!(parse_list_or_data(&json!({"id": "x"})).len(), 1);

        assert_eq!(parse_list_or_data(&json!({"data": {"id": "x"}})).len(), 1);
    }

    #[test]
    fn members_extract() {
        let resp = json!({"data": ["0xAAA", "0xBBB"]});
        assert_eq!(parse_members(&resp), vec!["0xAAA", "0xBBB"]);
        assert!(parse_members(&json!({})).is_empty());
    }

    #[test]
    fn active_project_filter() {
        assert!(project_is_active(&json!({"status": "in_progress"})));
        assert!(project_is_active(&json!({})));
        assert!(!project_is_active(&json!({"status": "finished"})));
        assert!(!project_is_active(&json!({"status": "revoked"})));
    }

    #[test]
    fn build_detail_merges_proposal_and_updates() {
        let project = json!({
            "id": "p-1",
            "proposal_id": "prop-1",
            "title": "My Grant",
            "status": "finished",
            "author": "0xauthor",
            "funding": { "vesting": { "total": 100, "vested": 100, "released": 100 } },
            "vesting_addresses": ["0xvest"]
        });
        let cfg = json!({
            "abstract": "Short summary.",
            "description": "Long description.",
            "personnel": "We are a team of builders.",
            "beneficiary": "0xbeneficiary",
            "milestones": [
                { "title": "M1", "tasks": "Do the thing", "delivery_date": "2025-01-01" },
                { "title": "M2", "delivery_date": "2025-02-01" }
            ]
        });
        let updates = vec![json!({ "id": "u1", "status": "done" })];

        let detail = build_project_detail(&project, Some(&cfg), &updates);

        assert_eq!(detail["id"], json!("p-1"));
        assert_eq!(detail["title"], json!("My Grant"));
        assert_eq!(detail["funding"]["vesting"]["total"], json!(100));
        assert_eq!(detail["vesting_addresses"], json!(["0xvest"]));

        assert_eq!(detail["about"], json!("Short summary."));

        assert_eq!(detail["links"], json!([]));

        let people = detail["personnel"].as_array().unwrap();
        assert_eq!(people.len(), 1);
        assert_eq!(people[0]["name"], json!("Team"));
        assert_eq!(people[0]["address"], json!("0xbeneficiary"));
        assert_eq!(people[0]["about"], json!("We are a team of builders."));

        let ms = detail["milestones"].as_array().unwrap();
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0]["id"], json!("m1"));
        assert_eq!(ms[0]["description"], json!("Do the thing"));
        assert_eq!(ms[0]["delivery_date"], json!("2025-01-01"));
        assert_eq!(ms[1]["description"], json!(""));

        assert_eq!(detail["updates"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn build_detail_empty_when_no_config() {
        let project = json!({ "id": "p-2", "author": "0xauthor" });
        let detail = build_project_detail(&project, None, &[]);
        assert_eq!(detail["about"], json!(""));
        assert_eq!(detail["links"], json!([]));
        assert_eq!(detail["personnel"], json!([]));
        assert_eq!(detail["milestones"], json!([]));
        assert_eq!(detail["updates"], json!([]));
    }

    #[test]
    fn build_detail_falls_back_to_description_and_roadmap() {
        let project = json!({ "id": "p-3" });
        let cfg = json!({
            "abstract": "   ",
            "description": "Use me instead.",
            "roadmap": [{ "title": "R1", "description": "legacy body" }]
        });
        let detail = build_project_detail(&project, Some(&cfg), &[]);
        assert_eq!(detail["about"], json!("Use me instead."));
        let ms = detail["milestones"].as_array().unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0]["title"], json!("R1"));
        assert_eq!(ms[0]["description"], json!("legacy body"));
        assert_eq!(ms[0]["delivery_date"], json!(null));
    }
}
