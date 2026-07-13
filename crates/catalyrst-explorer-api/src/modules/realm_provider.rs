use crate::AppState;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct AboutQuery {
    pub catalyst: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/main/about", get(main_about))
        .route("/about", get(main_about))
        .route("/realms", get(realms))
        .route("/hot-scenes", get(hot_scenes))
        .route("/status", get(status))
}

struct CatalystStatus {
    version: String,
    commit_hash: String,
    sync_state: String,
}

async fn fetch_catalyst_status(state: &AppState, base: &str) -> Option<CatalystStatus> {
    let url = format!("{}/content/status", base.trim_end_matches('/'));
    let resp = state.http.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        let url = format!("{}/status", base.trim_end_matches('/'));
        let resp = state.http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let v: Value = resp.json().await.ok()?;
        return Some(parse_catalyst_status(&v));
    }
    let v: Value = resp.json().await.ok()?;
    Some(parse_catalyst_status(&v))
}

fn parse_catalyst_status(v: &Value) -> CatalystStatus {
    let version = v
        .get("version")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty() && *s != "Unknown")
        .unwrap_or("")
        .to_string();
    let commit_hash = v
        .get("commitHash")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty() && *s != "Unknown")
        .unwrap_or("")
        .to_string();
    let sync_state = v
        .get("synchronizationStatus")
        .and_then(|s| s.get("synchronizationState"))
        .and_then(|x| x.as_str())
        .or_else(|| v.get("synchronizationStatus").and_then(|x| x.as_str()))
        .unwrap_or("Syncing")
        .to_string();
    CatalystStatus {
        version,
        commit_hash,
        sync_state,
    }
}

async fn main_about(State(state): State<AppState>, Query(q): Query<AboutQuery>) -> Json<Value> {
    let cfg = &state.cfg;
    let base = q
        .catalyst
        .as_deref()
        .map(|c| c.trim_end_matches('/').to_string())
        .unwrap_or_else(|| cfg.catalyst_url.trim_end_matches('/').to_string());

    let content_url = format!("{}/content", base);
    let lambdas_url = q
        .catalyst
        .as_deref()
        .map(|c| format!("{}/lambdas", c.trim_end_matches('/')))
        .unwrap_or_else(|| cfg.lambdas_url.trim_end_matches('/').to_string());

    let realm_name = cfg.realm_name.clone();
    let comms_adapter = cfg.comms_adapter.clone();
    let comms_fixed_adapter = cfg.comms_fixed_adapter.clone();
    let network_id = cfg.network_id;
    let pkg_version = env!("CARGO_PKG_VERSION");
    let commit_hash = option_env!("GIT_COMMIT").unwrap_or("");

    let catalyst = fetch_catalyst_status(&state, &base).await;
    let (content_version, content_commit, sync_state) = match &catalyst {
        Some(c) => (
            if c.version.is_empty() {
                pkg_version.to_string()
            } else {
                c.version.clone()
            },
            if c.commit_hash.is_empty() {
                commit_hash.to_string()
            } else {
                c.commit_hash.clone()
            },
            c.sync_state.clone(),
        ),
        None => (
            pkg_version.to_string(),
            commit_hash.to_string(),
            "Syncing".to_string(),
        ),
    };

    let body = json!({
        "healthy": true,
        "content": {
            "healthy": true,
            "version": content_version,
            "synchronizationStatus": sync_state,
            "commitHash": content_commit,
            "publicUrl": content_url,
        },
        "lambdas": {
            "healthy": true,
            "version": content_version,
            "commitHash": content_commit,
            "publicUrl": lambdas_url,
        },
        "configurations": {
            "networkId": network_id,
            "globalScenesUrn": [],
            "scenesUrn": [],
            "realmName": realm_name,
            "map": {
                "minimapEnabled": true,
                "sizes": [
                    { "left": -150, "top": 150, "right": 150, "bottom": -150 },
                    { "left": 62, "top": 158, "right": 162, "bottom": 151 },
                    { "left": 151, "top": 150, "right": 163, "bottom": 59 },
                ],
                "satelliteView": {
                    "version": "v1",
                    "baseUrl": std::env::var("MAP_SATELLITE_BASE_URL")
                        .ok()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "https://genesis.city/map/latest".to_string()),
                    "suffixUrl": ".jpg",
                    "topLeftOffset": { "x": -2, "y": -6 },
                },
                "parcelView": {
                    "version": "v1",
                    "imageUrl": std::env::var("MAP_PARCEL_VIEW_URL")
                        .ok()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "https://api.decentraland.org/v1/minimap.png".to_string()),
                },
            },
            "localSceneParcels": [],
            "skybox": { "fixedHour": -1 },
        },
        "bff": {
            "healthy": true,
            "protocolVersion": "1.0_0",
            "userCount": 0,
            "publicUrl": cfg.bff_url.clone(),
        },
        "acceptingUsers": true,
        "comms": {
            "version": pkg_version,
            "commitHash": commit_hash,
            "healthy": true,
            "protocol": "v3",
            "usersCount": 0,
            "adapter": comms_adapter,
            "fixedAdapter": comms_fixed_adapter,
        },
    });

    Json(body)
}

async fn realms(State(state): State<AppState>) -> Json<Value> {
    let cfg = &state.cfg;
    let realm_name = cfg.realm_name.clone();
    let url = cfg.public_realm_url.clone();
    let body = json!([
        {
            "serverName": realm_name,
            "url": url,
            "usersCount": 0,
        }
    ]);
    Json(body)
}

async fn hot_scenes(State(state): State<AppState>) -> Json<Value> {
    let url = &state.cfg.hot_scenes_url;
    match state.http.get(url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<Value>().await {
            Ok(v) => Json(v),
            Err(err) => {
                tracing::warn!(%url, %err, "hot-scenes upstream returned non-JSON; serving []");
                Json(json!([]))
            }
        },
        Ok(resp) => {
            tracing::warn!(%url, status = %resp.status(), "hot-scenes upstream error; serving []");
            Json(json!([]))
        }
        Err(err) => {
            tracing::warn!(%url, %err, "hot-scenes upstream unreachable; serving []");
            Json(json!([]))
        }
    }
}

async fn status() -> Json<Value> {
    let body = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "currentTime": chrono::Utc::now().timestamp_millis(),
        "commitHash": option_env!("GIT_COMMIT").unwrap_or(""),
    });
    Json(body)
}
