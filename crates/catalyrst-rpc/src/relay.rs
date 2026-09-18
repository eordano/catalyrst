use crate::state::{AppState, RpcMemo};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

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

struct Forward {
    id: Value,
    req: Value,
    memo: Option<(String, Duration)>,
}

enum Plan {
    Ready(Value),
    Forward(Forward),
}

fn memo_key(req: &Value, network: &str, method: &str) -> Option<(String, Duration)> {
    let ttl = RpcMemo::ttl_for(method)?;
    let no_params = match req.get("params") {
        None | Some(Value::Null) => true,
        Some(Value::Array(params)) => params.is_empty(),
        Some(_) => false,
    };
    no_params.then(|| (format!("{}:{method}", network.to_ascii_lowercase()), ttl))
}

fn plan(state: &AppState, network: &str, upstream: Option<&str>, req: Value) -> Plan {
    let id = id_of(&req);

    let method = match req.get("method").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return Plan::Ready(rpc_error(id, -32600, "Invalid Request: missing method")),
    };

    if !state.is_method_allowed(method) {
        return Plan::Ready(rpc_error(
            id,
            -32601,
            &format!("Method not allowed on read-only relay: {method}"),
        ));
    }

    if upstream.is_none() {
        return Plan::Ready(rpc_error(
            id,
            -32602,
            &format!("Unsupported network: {network}"),
        ));
    }

    let memo = memo_key(&req, network, method);
    if let Some((key, _)) = &memo {
        if let Some(body) = state.memo.get(key) {
            return Plan::Ready(normalize_response(id, body));
        }
    }
    Plan::Forward(Forward { id, req, memo })
}

fn remember(state: &AppState, memo: Option<(String, Duration)>, body: &Value) {
    if let Some((key, ttl)) = memo {
        state.memo.put(key, ttl, body);
    }
}

pub async fn handle_single(state: &AppState, network: &str, req: Value) -> Value {
    let upstream = state.upstream_for(network);
    match plan(state, network, upstream.as_deref(), req) {
        Plan::Ready(body) => body,
        Plan::Forward(item) => {
            let body = forward(state, upstream.as_deref().unwrap_or(""), item.id, item.req).await;
            remember(state, item.memo, &body);
            body
        }
    }
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

// One upstream batch with ids rewritten to slot indexes; anything the batch
// does not answer falls back to per-element forwards run concurrently.
async fn forward_batch(state: &AppState, upstream: &str, items: &[Forward]) -> Vec<Value> {
    if items.len() == 1 {
        return vec![forward(state, upstream, items[0].id.clone(), items[0].req.clone()).await];
    }
    let wire: Vec<Value> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let mut req = item.req.clone();
            if let Value::Object(map) = &mut req {
                map.insert("id".into(), json!(i));
            }
            req
        })
        .collect();
    let body = match state.http.post(upstream).json(&wire).send().await {
        Ok(r) => r.json::<Value>().await.ok(),
        Err(e) => {
            let msg = format!("Upstream request failed: {e}");
            return items
                .iter()
                .map(|item| rpc_error(item.id.clone(), -32603, &msg))
                .collect();
        }
    };
    let mut by_slot: HashMap<usize, Value> = HashMap::new();
    if let Some(Value::Array(elements)) = body {
        for element in elements {
            if let Some(slot) = element.get("id").and_then(Value::as_u64) {
                by_slot.insert(slot as usize, element);
            }
        }
    }
    let mut out: Vec<Option<Value>> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            by_slot
                .remove(&i)
                .map(|element| normalize_response(item.id.clone(), element))
        })
        .collect();
    let mut set = tokio::task::JoinSet::new();
    for (i, item) in items.iter().enumerate() {
        if out[i].is_some() {
            continue;
        }
        let state = state.clone();
        let upstream = upstream.to_string();
        let (id, req) = (item.id.clone(), item.req.clone());
        set.spawn(async move { (i, forward(&state, &upstream, id, req).await) });
    }
    while let Some(joined) = set.join_next().await {
        if let Ok((i, body)) = joined {
            out[i] = Some(body);
        }
    }
    out.into_iter()
        .zip(items)
        .map(|(body, item)| {
            body.unwrap_or_else(|| rpc_error(item.id.clone(), -32603, "Upstream relay task failed"))
        })
        .collect()
}

pub async fn handle_payload(state: &AppState, network: &str, payload: Value) -> Value {
    match payload {
        Value::Array(items) => {
            if items.is_empty() {
                return rpc_error(Value::Null, -32600, "Invalid Request: empty batch");
            }
            let upstream = state.upstream_for(network);
            let mut out: Vec<Option<Value>> = Vec::with_capacity(items.len());
            let mut slots: Vec<usize> = Vec::new();
            let mut forwards: Vec<Forward> = Vec::new();
            for (slot, item) in items.into_iter().enumerate() {
                match plan(state, network, upstream.as_deref(), item) {
                    Plan::Ready(body) => out.push(Some(body)),
                    Plan::Forward(item) => {
                        out.push(None);
                        slots.push(slot);
                        forwards.push(item);
                    }
                }
            }
            if !forwards.is_empty() {
                let bodies =
                    forward_batch(state, upstream.as_deref().unwrap_or(""), &forwards).await;
                for ((slot, item), body) in slots.into_iter().zip(forwards).zip(bodies) {
                    remember(state, item.memo, &body);
                    out[slot] = Some(body);
                }
            }
            Value::Array(out.into_iter().flatten().collect())
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
            memo: Default::default(),
        })
    }

    #[tokio::test]
    async fn unsupported_network_returns_invalid_params() {
        let state = test_state(&[("polygon", "https://example/polygon")]);
        let req = json!({ "jsonrpc": "2.0", "id": 5, "method": "eth_blockNumber" });
        let out = handle_single(&state, "solana", req).await;
        assert_eq!(out["jsonrpc"], json!("2.0"));
        assert_eq!(out["id"], json!(5));
        assert_eq!(out["error"]["code"], json!(-32602));
        assert!(
            out["error"]["message"].as_str().unwrap().contains("solana"),
            "error should name the offending network"
        );
    }

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    struct Upstream {
        posts: Arc<AtomicUsize>,
        payloads: Arc<Mutex<Vec<Value>>>,
    }

    fn answer(req: &Value) -> Value {
        let method = req["method"].as_str().unwrap_or("");
        let result = match method {
            "eth_blockNumber" => json!("0x10"),
            "net_version" => json!("137"),
            "eth_getBalance" => json!(format!("0xbal{}", req["params"][0].as_str().unwrap_or(""))),
            _ => json!(null),
        };
        json!({ "jsonrpc": "2.0", "id": req["id"], "result": result })
    }

    async fn mock_upstream(batches: bool) -> (String, Upstream) {
        use axum::{routing::post, Json, Router};
        let posts = Arc::new(AtomicUsize::new(0));
        let payloads = Arc::new(Mutex::new(Vec::new()));
        let (p, l) = (posts.clone(), payloads.clone());
        let app = Router::new().route(
            "/",
            post(move |Json(payload): Json<Value>| {
                let (p, l) = (p.clone(), l.clone());
                async move {
                    p.fetch_add(1, Ordering::SeqCst);
                    l.lock().unwrap().push(payload.clone());
                    let reply = match &payload {
                        Value::Array(items) if batches => {
                            Value::Array(items.iter().map(answer).collect())
                        }
                        Value::Array(_) => json!({
                            "jsonrpc": "2.0", "id": null,
                            "error": { "code": -32600, "message": "batch not supported" }
                        }),
                        single => answer(single),
                    };
                    Json(reply)
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}/"), Upstream { posts, payloads })
    }

    fn batch_payload() -> Value {
        json!([
            { "jsonrpc": "2.0", "id": "a", "method": "eth_getBalance", "params": ["0x1", "latest"] },
            { "jsonrpc": "2.0", "id": 7, "method": "eth_sendRawTransaction", "params": ["0x"] },
            { "jsonrpc": "2.0", "id": "a", "method": "eth_getBalance", "params": ["0x2", "latest"] },
            { "jsonrpc": "2.0", "method": "net_version" },
        ])
    }

    fn check_batch(out: &Value) {
        let out = out.as_array().unwrap();
        assert_eq!(out.len(), 4);
        assert_eq!(out[0]["id"], json!("a"));
        assert_eq!(out[0]["result"], json!("0xbal0x1"));
        assert_eq!(out[1]["id"], json!(7));
        assert_eq!(out[1]["error"]["code"], json!(-32601));
        assert_eq!(out[2]["id"], json!("a"));
        assert_eq!(out[2]["result"], json!("0xbal0x2"));
        assert_eq!(out[3]["id"], json!(null));
        assert_eq!(out[3]["result"], json!("137"));
        for item in out {
            assert_eq!(item["jsonrpc"], json!("2.0"));
        }
    }

    #[tokio::test]
    async fn batch_is_one_upstream_post_with_ids_echoed() {
        let (url, upstream) = mock_upstream(true).await;
        let state = test_state(&[("polygon", &url)]);
        let out = handle_payload(&state, "polygon", batch_payload()).await;
        check_batch(&out);
        assert_eq!(upstream.posts.load(Ordering::SeqCst), 1);
        let sent = upstream.payloads.lock().unwrap();
        let wire = sent[0].as_array().unwrap();
        assert_eq!(
            wire.len(),
            3,
            "the locally rejected element never goes upstream"
        );
        assert_eq!(wire[0]["id"], json!(0));
        assert_eq!(wire[2]["id"], json!(2));
        assert_eq!(wire[2]["method"], json!("net_version"));
    }

    #[tokio::test]
    async fn batch_falls_back_per_element_when_upstream_rejects_batches() {
        let (url, upstream) = mock_upstream(false).await;
        let state = test_state(&[("polygon", &url)]);
        let out = handle_payload(&state, "polygon", batch_payload()).await;
        check_batch(&out);
        assert_eq!(upstream.posts.load(Ordering::SeqCst), 4);
        let sent = upstream.payloads.lock().unwrap();
        assert!(sent[1..].iter().all(|p| p.is_object()));
        assert_eq!(
            sent[1..].iter().filter(|p| p["id"] == json!("a")).count(),
            2
        );
    }

    #[tokio::test]
    async fn block_number_is_memoized_briefly() {
        let (url, upstream) = mock_upstream(true).await;
        let state = test_state(&[("polygon", &url)]);
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "eth_blockNumber", "params": [] });
        let first = handle_single(&state, "polygon", req.clone()).await;
        assert_eq!(first["result"], json!("0x10"));
        let second = handle_single(
            &state,
            "POLYGON",
            json!({ "id": 2, "method": "eth_blockNumber" }),
        )
        .await;
        assert_eq!(second["id"], json!(2));
        assert_eq!(second["result"], json!("0x10"));
        assert_eq!(upstream.posts.load(Ordering::SeqCst), 1);
        let batch = handle_payload(&state, "polygon", json!([req])).await;
        assert_eq!(batch[0]["result"], json!("0x10"));
        assert_eq!(upstream.posts.load(Ordering::SeqCst), 1);
        let with_params =
            json!({ "id": 3, "method": "eth_getBalance", "params": ["0x1", "latest"] });
        handle_single(&state, "polygon", with_params).await;
        assert_eq!(upstream.posts.load(Ordering::SeqCst), 2);
    }
}
