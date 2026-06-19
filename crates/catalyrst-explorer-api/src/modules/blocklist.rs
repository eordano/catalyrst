use crate::modules::admin_auth::require_admin;
use crate::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use std::fmt;
use tokio::fs;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Denylist {
    pub users: Vec<UserEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserEntry {
    pub wallet: String,
}

impl<'de> Deserialize<'de> for Denylist {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default, deserialize_with = "de_users")]
            users: Vec<UserEntry>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Denylist { users: raw.users })
    }
}

fn de_users<'de, D>(deserializer: D) -> Result<Vec<UserEntry>, D::Error>
where
    D: Deserializer<'de>,
{
    struct UsersVisitor;
    impl<'de> Visitor<'de> for UsersVisitor {
        type Value = Vec<UserEntry>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("an array of wallet strings or {wallet} objects")
        }
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            #[derive(Deserialize)]
            #[serde(untagged)]
            enum Elem {
                Str(String),
                Obj { wallet: String },
            }
            let mut out = Vec::new();
            while let Some(elem) = seq.next_element::<Elem>()? {
                match elem {
                    Elem::Str(wallet) => out.push(UserEntry { wallet }),
                    Elem::Obj { wallet } => out.push(UserEntry { wallet }),
                }
            }
            Ok(out)
        }
    }
    deserializer.deserialize_seq(UsersVisitor)
}

#[derive(Debug, Deserialize)]
pub struct WalletBody {
    pub wallet: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/denylist.json", get(get_denylist))
        // Admin (bearer-gated) controls. The on-disk file at `blocklist_path` is
        // the source of truth (the public GET reads it on each request), so these
        // mutate the file directly and remain consistent with the read path.
        .route("/admin/blocklist/add", post(admin_add))
        .route("/admin/blocklist/remove", post(admin_remove))
        .route("/admin/blocklist/reload", post(admin_reload))
}

fn normalize_wallet(w: &str) -> String {
    w.trim().to_lowercase()
}

/// Read the current denylist from disk, returning an empty list if the file is
/// missing/unparseable (matching the public read path's tolerance).
async fn read_denylist(path: &str) -> Denylist {
    match fs::read(path).await {
        Ok(bytes) => serde_json::from_slice::<Denylist>(&bytes).unwrap_or_default(),
        Err(_) => Denylist::default(),
    }
}

/// Serialize a denylist as `{ "users": [{ "wallet": ... }] }` and write it to
/// disk atomically (write tmp then rename) so a concurrent reader never sees a
/// truncated file.
async fn write_denylist(path: &str, list: &Denylist) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(&json!({ "users": list.users }))
        .map_err(|e| e.to_string())?;
    let tmp = format!("{path}.tmp");
    fs::write(&tmp, &body).await.map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).await.map_err(|e| e.to_string())
}

async fn admin_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<WalletBody>,
) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let wallet = normalize_wallet(&body.wallet);
    if wallet.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "wallet is required" })),
        )
            .into_response();
    }
    let path = state.cfg.blocklist_path.clone();
    let mut list = read_denylist(&path).await;
    let already = list.users.iter().any(|u| normalize_wallet(&u.wallet) == wallet);
    if !already {
        list.users.push(UserEntry { wallet: wallet.clone() });
        if let Err(err) = write_denylist(&path, &list).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "ok": false, "error": err })),
            )
                .into_response();
        }
    }
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "wallet": wallet, "added": !already, "count": list.users.len() })),
    )
        .into_response()
}

async fn admin_remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<WalletBody>,
) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let wallet = normalize_wallet(&body.wallet);
    if wallet.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "wallet is required" })),
        )
            .into_response();
    }
    let path = state.cfg.blocklist_path.clone();
    let mut list = read_denylist(&path).await;
    let before = list.users.len();
    list.users.retain(|u| normalize_wallet(&u.wallet) != wallet);
    let removed = list.users.len() != before;
    if removed {
        if let Err(err) = write_denylist(&path, &list).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "ok": false, "error": err })),
            )
                .into_response();
        }
    }
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "wallet": wallet, "removed": removed, "count": list.users.len() })),
    )
        .into_response()
}

/// Reload-from-disk: re-read the file and return its current contents. Because
/// the public GET reads the file on every request there is no in-memory cache to
/// invalidate; this confirms the file parses and reports the active entry count.
async fn admin_reload(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let path = state.cfg.blocklist_path.clone();
    match fs::read(&path).await {
        Ok(bytes) => match serde_json::from_slice::<Denylist>(&bytes) {
            Ok(list) => (
                StatusCode::OK,
                Json(json!({ "ok": true, "path": path, "count": list.users.len() })),
            )
                .into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "ok": false, "path": path, "error": err.to_string() })),
            )
                .into_response(),
        },
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "ok": false, "path": path, "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn get_denylist(State(state): State<AppState>) -> impl IntoResponse {
    let path = &state.cfg.blocklist_path;
    match fs::read(path).await {
        Ok(bytes) => match serde_json::from_slice::<Denylist>(&bytes) {
            Ok(list) => (StatusCode::OK, Json(list)).into_response(),
            Err(err) => {
                tracing::warn!(?path, %err, "denylist parse failed; serving empty");
                (StatusCode::OK, Json(Denylist::default())).into_response()
            }
        },
        Err(err) => {
            tracing::warn!(?path, %err, "denylist read failed; serving empty");
            (StatusCode::OK, Json(Denylist::default())).into_response()
        }
    }
}
