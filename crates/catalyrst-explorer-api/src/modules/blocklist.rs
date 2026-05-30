use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
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

pub fn routes() -> Router<AppState> {
    Router::new().route("/denylist.json", get(get_denylist))
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
