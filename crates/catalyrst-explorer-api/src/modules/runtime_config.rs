//! In-memory runtime config store.
//!
//! The crate's `Config` (see `config.rs`) is loaded once from env at startup and
//! is immutable thereafter. This module adds a separate, bearer-gated key/value
//! store that lives only in memory (an `RwLock<BTreeMap>`), so an operator can
//! stash runtime overrides / notes that the console can read back during a
//! process lifetime. It is intentionally *not* persisted and does not mutate the
//! startup `Config` — nothing in the request path reads it yet, so there is no
//! risk of it silently changing realm/upstream behavior. It is a durable place
//! for the console to record operator intent until per-key wiring lands.

use crate::modules::admin_auth::require_admin;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use parking_lot::RwLock;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct RuntimeConfigState {
    inner: RwLock<BTreeMap<String, Value>>,
}

impl RuntimeConfigState {
    pub fn snapshot(&self) -> BTreeMap<String, Value> {
        self.inner.read().clone()
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        self.inner.read().get(key).cloned()
    }

    pub fn set(&self, key: String, value: Value) {
        self.inner.write().insert(key, value);
    }

    pub fn remove(&self, key: &str) -> Option<Value> {
        self.inner.write().remove(key)
    }
}

#[derive(Debug, Deserialize)]
pub struct SetBody {
    pub value: Value,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/config", get(list_config))
        .route(
            "/admin/config/{key}",
            get(get_config).put(set_config).delete(delete_config),
        )
}

async fn list_config(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let map = state.runtime_config.snapshot();
    (StatusCode::OK, Json(json!({ "config": map }))).into_response()
}

async fn get_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    match state.runtime_config.get(&key) {
        Some(value) => {
            (StatusCode::OK, Json(json!({ "key": key, "value": value }))).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not_found", "key": key })),
        )
            .into_response(),
    }
}

async fn set_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Json(body): Json<SetBody>,
) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    state.runtime_config.set(key.clone(), body.value.clone());
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "key": key, "value": body.value })),
    )
        .into_response()
}

async fn delete_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let removed = state.runtime_config.remove(&key);
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "key": key, "removed": removed.is_some() })),
    )
        .into_response()
}
