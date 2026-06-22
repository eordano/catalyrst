use crate::state::AppState;
use serde_json::{json, Value};

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

fn normalize_response(id: Value, body: Value) -> Value {
    let Value::Object(mut map) = body else {
        return rpc_error(
            id,
            -32603,
            "Upstream returned a non-object JSON-RPC response",
        );
    };

    map.insert("jsonrpc".into(), Value::String("2.0".into()));
    map.insert("id".into(), id.clone());

    let has_result = map.contains_key("result");
    let has_error = map.contains_key("error");
    if !has_result && !has_error {
        return rpc_error(
            id,
            -32603,
            "Upstream response carried neither result nor error",
        );
    }

    Value::Object(map)
}

fn id_of(req: &Value) -> Value {
    req.get("id").cloned().unwrap_or(Value::Null)
}

pub async fn handle_single(state: &AppState, network: &str, req: Value) -> Value {
    let id = id_of(&req);

    let method = match req.get("method").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return rpc_error(id, -32600, "Invalid Request: missing method"),
    };

    if !state.is_method_allowed(method) {
        return rpc_error(
            id,
            -32601,
            &format!("Method not allowed on read-only relay: {method}"),
        );
    }

    let upstream = match state.upstream_for(network) {
        Some(u) => u,
        None => {
            return rpc_error(id, -32602, &format!("Unsupported network: {network}"));
        }
    };

    forward(state, &upstream, id, req).await
}

async fn forward(state: &AppState, upstream: &str, id: Value, req: Value) -> Value {
    let resp = state.http.post(upstream).json(&req).send().await;
    match resp {
        Ok(r) => match r.json::<Value>().await {
            Ok(body) => normalize_response(id, body),
            Err(e) => rpc_error(id, -32603, &format!("Upstream returned invalid JSON: {e}")),
        },
        Err(e) => rpc_error(id, -32603, &format!("Upstream request failed: {e}")),
    }
}

pub async fn handle_payload(state: &AppState, network: &str, payload: Value) -> Value {
    match payload {
        Value::Array(items) => {
            if items.is_empty() {
                return rpc_error(Value::Null, -32600, "Invalid Request: empty batch");
            }
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(handle_single(state, network, item).await);
            }
            Value::Array(out)
        }
        single @ Value::Object(_) => handle_single(state, network, single).await,
        other => rpc_error(
            id_of(&other),
            -32600,
            "Invalid Request: expected object or array",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_envelope_is_jsonrpc_2_0_conformant() {
        let e = rpc_error(json!(7), -32601, "nope");
        assert_eq!(e["jsonrpc"], json!("2.0"));
        assert_eq!(e["id"], json!(7));
        assert_eq!(e["error"]["code"], json!(-32601));
        assert!(
            e.get("result").is_none(),
            "error response must not carry result"
        );
    }

    #[test]
    fn normalize_forces_jsonrpc_and_echoes_id() {
        let upstream = json!({ "id": "abc", "result": "0x1" });
        let out = normalize_response(json!(42), upstream);
        assert_eq!(out["jsonrpc"], json!("2.0"));
        assert_eq!(out["id"], json!(42));
        assert_eq!(out["result"], json!("0x1"));
        assert!(out.get("error").is_none());
    }

    #[test]
    fn normalize_preserves_upstream_error() {
        let upstream = json!({ "error": { "code": -32000, "message": "reverted" } });
        let out = normalize_response(json!(1), upstream);
        assert_eq!(out["jsonrpc"], json!("2.0"));
        assert_eq!(out["id"], json!(1));
        assert_eq!(out["error"]["code"], json!(-32000));
        assert!(out.get("result").is_none());
    }

    #[test]
    fn normalize_repairs_resultless_and_errorless_body() {
        let out = normalize_response(json!(3), json!({ "foo": "bar" }));
        assert_eq!(out["jsonrpc"], json!("2.0"));
        assert_eq!(out["id"], json!(3));
        assert_eq!(out["error"]["code"], json!(-32603));
    }

    #[test]
    fn normalize_rejects_non_object_body() {
        let out = normalize_response(json!(9), json!(["not", "an", "object"]));
        assert_eq!(out["jsonrpc"], json!("2.0"));
        assert_eq!(out["id"], json!(9));
        assert_eq!(out["error"]["code"], json!(-32603));
    }

    fn test_state(networks: &[(&str, &str)]) -> AppState {
        use crate::state::{AppStateInner, READ_ONLY_METHODS};
        use std::collections::{BTreeMap, BTreeSet};
        use std::sync::{Arc, RwLock};

        let allowed_methods: BTreeSet<String> =
            READ_ONLY_METHODS.iter().map(|m| m.to_string()).collect();
        let entries: Vec<(String, String)> = networks
            .iter()
            .map(|(n, u)| (n.to_string(), u.to_string()))
            .collect();
        Arc::new(AppStateInner {
            cfg: crate::Config {
                http_host: "127.0.0.1".into(),
                http_port: 0,
                upstreams: entries.iter().cloned().collect(),
            },
            http: reqwest::Client::new(),
            allowed_methods: RwLock::new(allowed_methods),
            upstreams: RwLock::new(entries.into_iter().collect::<BTreeMap<_, _>>()),
            admin_token: None,
        })
    }

    #[tokio::test]
    async fn unsupported_network_returns_invalid_params() {
        // A genuinely-unknown network is rejected with -32602 before any
        // upstream call, regardless of how many networks are configured.
        let state = test_state(&[("polygon", "https://example/polygon")]);
        let req = json!({ "jsonrpc": "2.0", "id": 5, "method": "eth_blockNumber" });
        let out = handle_single(&state, "solana", req).await;
        assert_eq!(out["jsonrpc"], json!("2.0"));
        assert_eq!(out["id"], json!(5));
        assert_eq!(out["error"]["code"], json!(-32602));
        assert!(
            out["error"]["message"]
                .as_str()
                .unwrap()
                .contains("solana"),
            "error should name the offending network"
        );
    }
}
