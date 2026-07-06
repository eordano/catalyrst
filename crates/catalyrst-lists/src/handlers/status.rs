use axum::Json;
use serde_json::{json, Value};

pub async fn status() -> Json<Value> {
    let commit_hash = option_env!("GIT_REV").unwrap_or(env!("CARGO_PKG_VERSION"));
    Json(json!({ "commitHash": commit_hash }))
}
