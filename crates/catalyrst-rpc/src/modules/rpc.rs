use crate::relay;
use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

pub fn routes() -> Router<AppState> {
    Router::new().route("/{network}", post(http_rpc).get(ws_upgrade))
}

async fn http_rpc(
    Path(network): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let resp = relay::handle_payload(&state, &network, payload).await;
    (StatusCode::OK, Json(resp))
}

async fn ws_upgrade(
    Path(network): Path<String>,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, network))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, network: String) {
    while let Some(incoming) = socket.recv().await {
        let msg = match incoming {
            Ok(m) => m,
            Err(_) => break,
        };
        match msg {
            Message::Text(t) => {
                let reply = match serde_json::from_str::<Value>(&t) {
                    Ok(payload) => relay::handle_payload(&state, &network, payload).await,
                    Err(e) => json!({
                        "jsonrpc": "2.0",
                        "id": Value::Null,
                        "error": { "code": -32700, "message": format!("Parse error: {e}") },
                    }),
                };
                let out = reply.to_string();
                if socket.send(Message::Text(out.into())).await.is_err() {
                    break;
                }
            }
            Message::Ping(ref p) if socket.send(Message::Pong(p.clone())).await.is_err() => {
                break;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}
