use crate::modules::admin_auth::require_admin;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use parking_lot::RwLock;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path as StdPath;

const EMBEDDED_FLAGS: &str = include_str!("../../assets/feature-flags.explorer.json");

pub struct FeatureFlagsState {
    inner: RwLock<Value>,
}

impl Default for FeatureFlagsState {
    fn default() -> Self {
        match std::env::var("FEATURE_FLAGS_CONFIG_PATH") {
            Ok(p) if !p.is_empty() && StdPath::new(&p).exists() => Self::load_from_path(p),
            _ => Self {
                inner: RwLock::new(default_payload()),
            },
        }
    }
}

impl FeatureFlagsState {
    pub fn load_from_path<P: AsRef<StdPath>>(path: P) -> Self {
        let value = match std::fs::read(path.as_ref()) {
            Ok(bytes) => normalize_payload(serde_json::from_slice::<Value>(&bytes).unwrap_or_else(
                |err| {
                    tracing::warn!(path = ?path.as_ref(), %err, "feature-flags parse failed; using embedded default");
                    default_payload()
                },
            )),
            Err(err) => {
                tracing::warn!(path = ?path.as_ref(), %err, "feature-flags read failed; using embedded default");
                default_payload()
            }
        };
        Self {
            inner: RwLock::new(value),
        }
    }

    pub fn snapshot(&self) -> Value {
        self.inner.read().clone()
    }

    /// Set a single flag's value under `flags.<name>`. Returns the new value.
    /// Used by the admin toggle route; mutates only the in-memory `RwLock`
    /// (the on-disk config file is the reload source and is left untouched).
    pub fn set_flag(&self, name: &str, value: Value) -> Value {
        let mut guard = self.inner.write();
        if let Some(flags) = guard
            .get_mut("flags")
            .and_then(Value::as_object_mut)
        {
            flags.insert(name.to_string(), value.clone());
        }
        value
    }

    /// Re-read the flags payload from `path` (the configured
    /// `FEATURE_FLAGS_CONFIG_PATH`) and replace the in-memory snapshot.
    /// Returns `Ok(())` on a successful read+parse; on read/parse failure the
    /// existing snapshot is kept and an error message is returned so the caller
    /// can surface it (we do NOT silently fall back to the embedded default,
    /// which would clobber live state).
    pub fn reload_from_path<P: AsRef<StdPath>>(&self, path: P) -> Result<(), String> {
        let bytes = std::fs::read(path.as_ref()).map_err(|e| e.to_string())?;
        let parsed: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        *self.inner.write() = normalize_payload(parsed);
        Ok(())
    }
}

fn default_payload() -> Value {
    normalize_payload(serde_json::from_str::<Value>(EMBEDDED_FLAGS).unwrap_or_else(|_| {
        json!({
            "flags": {},
            "variants": {},
        })
    }))
}

fn normalize_payload(mut value: Value) -> Value {
    if !value.is_object() {
        return json!({ "flags": {}, "variants": {} });
    }
    let obj = value.as_object_mut().unwrap();
    if !obj.get("flags").map(Value::is_object).unwrap_or(false) {
        obj.insert("flags".into(), json!({}));
    }
    if !obj.get("variants").map(Value::is_object).unwrap_or(false) {
        obj.insert("variants".into(), json!({}));
    }
    obj.retain(|k, _| k == "flags" || k == "variants");
    value
}

#[derive(Debug, Deserialize)]
pub struct FlagToggleBody {
    pub name: String,
    /// New value for the flag. Defaults to a boolean toggle when omitted is not
    /// possible (no prior value semantics), so callers should pass an explicit
    /// value; absent => `true`.
    #[serde(default = "default_flag_value")]
    pub value: Value,
}

fn default_flag_value() -> Value {
    Value::Bool(true)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{app_name}", get(get_app_json))
        .route("/flags/{name}", get(get_flag))
        // Admin (bearer-gated) controls — additive, do not affect the GET routes.
        .route("/admin/flags/toggle", post(admin_flag_toggle))
        .route("/admin/flags/reload", post(admin_flags_reload))
}

async fn admin_flag_toggle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<FlagToggleBody>,
) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    if body.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "name is required" })),
        )
            .into_response();
    }
    let new_value = state.feature_flags.set_flag(&body.name, body.value);
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "name": body.name, "value": new_value })),
    )
        .into_response()
}

async fn admin_flags_reload(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let path = state.cfg.feature_flags_config_path.clone();
    match state.feature_flags.reload_from_path(&path) {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "ok": true, "path": path })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "ok": false, "path": path, "error": err })),
        )
            .into_response(),
    }
}

async fn get_app_json(
    State(state): State<AppState>,
    Path(_app_name): Path<String>,
) -> impl IntoResponse {
    Json(state.feature_flags.snapshot())
}

async fn get_flag(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let snap = state.feature_flags.snapshot();
    if let Some(variant) = snap.get("variants").and_then(|v| v.get(&name)) {
        return (StatusCode::OK, Json(variant.clone())).into_response();
    }
    if let Some(flag) = snap.get("flags").and_then(|v| v.get(&name)) {
        return (StatusCode::OK, Json(flag.clone())).into_response();
    }
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "flag_not_found", "name": name })),
    )
        .into_response()
}
