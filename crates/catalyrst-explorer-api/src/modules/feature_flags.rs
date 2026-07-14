use crate::modules::admin_auth::require_admin;
use crate::modules::{json_response, ErrorMessage};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path as StdPath;
use std::sync::Arc;

const EMBEDDED_FLAGS: &str = include_str!("../../assets/feature-flags.explorer.json");

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureFlagsDocument {
    #[serde(default)]
    pub flags: BTreeMap<String, bool>,
    #[serde(default)]
    pub variants: BTreeMap<String, Variant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<VariantPayload>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantPayload {
    #[serde(rename = "type")]
    pub r#type: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct FlagToggleAck {
    pub ok: bool,
    pub name: String,
    pub value: bool,
}

#[derive(Serialize)]
pub struct ReloadAck {
    pub ok: bool,
    pub path: String,
}

#[derive(Serialize)]
pub struct ReloadError {
    pub ok: bool,
    pub path: String,
    pub error: String,
}

#[derive(Serialize)]
pub struct FlagNotFound {
    pub error: &'static str,
    pub name: String,
}

pub struct FeatureFlagsState {
    inner: RwLock<Arc<FeatureFlagsDocument>>,
}

impl Default for FeatureFlagsState {
    fn default() -> Self {
        match std::env::var("FEATURE_FLAGS_CONFIG_PATH") {
            Ok(p) if !p.is_empty() && StdPath::new(&p).exists() => Self::load_from_path(p),
            _ => Self {
                inner: RwLock::new(Arc::new(default_payload())),
            },
        }
    }
}

impl FeatureFlagsState {
    pub fn load_from_path<P: AsRef<StdPath>>(path: P) -> Self {
        let doc = match std::fs::read(path.as_ref()) {
            Ok(bytes) => serde_json::from_slice::<FeatureFlagsDocument>(&bytes).unwrap_or_else(
                |err| {
                    tracing::warn!(path = ?path.as_ref(), %err, "feature-flags parse failed; using embedded default");
                    default_payload()
                },
            ),
            Err(err) => {
                tracing::warn!(path = ?path.as_ref(), %err, "feature-flags read failed; using embedded default");
                default_payload()
            }
        };
        Self {
            inner: RwLock::new(Arc::new(doc)),
        }
    }

    pub fn snapshot(&self) -> Arc<FeatureFlagsDocument> {
        Arc::clone(&*self.inner.read())
    }

    pub fn set_flag(&self, name: &str, value: bool) -> bool {
        let mut guard = self.inner.write();
        let mut next = (**guard).clone();
        next.flags.insert(name.to_string(), value);
        *guard = Arc::new(next);
        value
    }

    pub fn reload_from_path<P: AsRef<StdPath>>(&self, path: P) -> Result<(), String> {
        let bytes = std::fs::read(path.as_ref()).map_err(|e| e.to_string())?;
        let doc: FeatureFlagsDocument =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        *self.inner.write() = Arc::new(doc);
        Ok(())
    }
}

fn default_payload() -> FeatureFlagsDocument {
    serde_json::from_str::<FeatureFlagsDocument>(EMBEDDED_FLAGS).unwrap_or_default()
}

#[derive(Debug, Deserialize)]
pub struct FlagToggleBody {
    pub name: String,

    #[serde(default = "default_flag_value")]
    pub value: bool,
}

fn default_flag_value() -> bool {
    true
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{app_name}", get(get_app_json))
        .route("/flags/{name}", get(get_flag))
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
        return json_response(
            StatusCode::BAD_REQUEST,
            ErrorMessage {
                error: "name is required".to_string(),
            },
        );
    }
    let new_value = state.feature_flags.set_flag(&body.name, body.value);
    json_response(
        StatusCode::OK,
        FlagToggleAck {
            ok: true,
            name: body.name,
            value: new_value,
        },
    )
}

async fn admin_flags_reload(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let path = state.cfg.feature_flags_config_path.clone();
    match state.feature_flags.reload_from_path(&path) {
        Ok(()) => json_response(StatusCode::OK, ReloadAck { ok: true, path }),
        Err(error) => json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ReloadError {
                ok: false,
                path,
                error,
            },
        ),
    }
}

async fn get_app_json(
    State(state): State<AppState>,
    Path(_app_name): Path<String>,
) -> impl IntoResponse {
    Json(state.feature_flags.snapshot())
}

async fn get_flag(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let snap = state.feature_flags.snapshot();
    if let Some(variant) = snap.variants.get(&name) {
        return json_response(StatusCode::OK, variant);
    }
    if let Some(flag) = snap.flags.get(&name) {
        return json_response(StatusCode::OK, *flag);
    }
    json_response(
        StatusCode::NOT_FOUND,
        FlagNotFound {
            error: "flag_not_found",
            name,
        },
    )
}
