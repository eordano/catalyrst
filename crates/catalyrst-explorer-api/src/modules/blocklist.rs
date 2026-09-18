use crate::modules::admin_auth::require_admin;
use crate::modules::{json_response, ErrorMessage};
use crate::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;
use tokio::fs;

#[derive(Serialize)]
pub struct BlocklistAddAck {
    pub ok: bool,
    pub wallet: String,
    pub added: bool,
    pub count: u64,
}

#[derive(Serialize)]
pub struct BlocklistRemoveAck {
    pub ok: bool,
    pub wallet: String,
    pub removed: bool,
    pub count: u64,
}

#[derive(Serialize)]
pub struct BlocklistReloadAck {
    pub ok: bool,
    pub path: String,
    pub count: u64,
}

#[derive(Serialize)]
pub struct BlocklistError {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub error: String,
}

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

/// The served list plus a normalized wallet set, so admin edits never re-read the file.
pub struct DenylistCache {
    list: Arc<Denylist>,
    wallets: HashSet<String>,
}

impl DenylistCache {
    pub fn new(list: Denylist) -> Self {
        let wallets = list
            .users
            .iter()
            .map(|u| normalize_wallet(&u.wallet))
            .collect();
        Self {
            list: Arc::new(list),
            wallets,
        }
    }

    pub fn list(&self) -> Arc<Denylist> {
        self.list.clone()
    }

    pub fn contains(&self, wallet: &str) -> bool {
        self.wallets.contains(wallet)
    }
}

#[derive(Debug, Deserialize)]
pub struct WalletBody {
    pub wallet: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/denylist.json", get(get_denylist))
        .route("/admin/blocklist/add", post(admin_add))
        .route("/admin/blocklist/remove", post(admin_remove))
        .route("/admin/blocklist/reload", post(admin_reload))
}

fn normalize_wallet(w: &str) -> String {
    w.trim().to_lowercase()
}

pub(crate) async fn read_denylist(path: &str) -> Denylist {
    match fs::read(path).await {
        Ok(bytes) => serde_json::from_slice::<Denylist>(&bytes).unwrap_or_default(),
        Err(_) => Denylist::default(),
    }
}

async fn write_denylist(path: &str, list: &Denylist) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(list).map_err(|e| e.to_string())?;
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
        return json_response(
            StatusCode::BAD_REQUEST,
            ErrorMessage {
                error: "wallet is required".to_string(),
            },
        );
    }
    let _serialized = state.denylist_write.lock().await;
    let current = state.denylist.read().list();
    let already = state.denylist.read().contains(&wallet);
    let count = if already {
        current.users.len() as u64
    } else {
        let mut list = (*current).clone();
        list.users.push(UserEntry {
            wallet: wallet.clone(),
        });
        if let Err(error) = write_denylist(&state.cfg.blocklist_path, &list).await {
            return json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                BlocklistError {
                    ok: false,
                    path: None,
                    error,
                },
            );
        }
        let count = list.users.len() as u64;
        *state.denylist.write() = DenylistCache::new(list);
        count
    };
    json_response(
        StatusCode::OK,
        BlocklistAddAck {
            ok: true,
            wallet,
            added: !already,
            count,
        },
    )
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
        return json_response(
            StatusCode::BAD_REQUEST,
            ErrorMessage {
                error: "wallet is required".to_string(),
            },
        );
    }
    let _serialized = state.denylist_write.lock().await;
    let current = state.denylist.read().list();
    let removed = state.denylist.read().contains(&wallet);
    let count = if !removed {
        current.users.len() as u64
    } else {
        let mut list = (*current).clone();
        list.users.retain(|u| normalize_wallet(&u.wallet) != wallet);
        if let Err(error) = write_denylist(&state.cfg.blocklist_path, &list).await {
            return json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                BlocklistError {
                    ok: false,
                    path: None,
                    error,
                },
            );
        }
        let count = list.users.len() as u64;
        *state.denylist.write() = DenylistCache::new(list);
        count
    };
    json_response(
        StatusCode::OK,
        BlocklistRemoveAck {
            ok: true,
            wallet,
            removed,
            count,
        },
    )
}

async fn admin_reload(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let path = state.cfg.blocklist_path.clone();
    match fs::read(&path).await {
        Ok(bytes) => match serde_json::from_slice::<Denylist>(&bytes) {
            Ok(list) => {
                let count = list.users.len() as u64;
                *state.denylist.write() = DenylistCache::new(list);
                json_response(
                    StatusCode::OK,
                    BlocklistReloadAck {
                        ok: true,
                        path,
                        count,
                    },
                )
            }
            Err(err) => json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                BlocklistError {
                    ok: false,
                    path: Some(path),
                    error: err.to_string(),
                },
            ),
        },
        Err(err) => json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            BlocklistError {
                ok: false,
                path: Some(path),
                error: err.to_string(),
            },
        ),
    }
}

async fn get_denylist(State(state): State<AppState>) -> impl IntoResponse {
    let list = state.denylist.read().list();
    (StatusCode::OK, Json(list)).into_response()
}
