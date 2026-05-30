use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;
use tokio::sync::Mutex as AsyncMutex;

use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutResponse {
    pub healthy: bool,
    pub content: AboutContent,
    pub lambdas: AboutLambdas,
    pub configurations: AboutConfigurations,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comms: Option<AboutComms>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bff: Option<AboutBff>,
    pub accepting_users: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutContent {
    pub healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub synchronization_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    pub public_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutLambdas {
    pub healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    pub public_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutConfigurations {
    pub network_id: u64,
    pub global_scenes_urn: Vec<String>,
    pub scenes_urn: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realm_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<AboutConfigurationsMap>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutConfigurationsMap {
    pub minimap_enabled: bool,
    pub sizes: Vec<AboutMapSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub satellite_view: Option<AboutMapView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parcel_view: Option<AboutParcelView>,
}

#[derive(Serialize)]
pub struct AboutMapSize {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutMapView {
    pub version: String,
    pub base_url: String,
    pub suffix_url: String,
    pub top_left_offset: AboutMapOffset,
}

#[derive(Serialize)]
pub struct AboutMapOffset {
    pub x: i32,
    pub y: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutParcelView {
    pub version: String,
    pub image_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutComms {
    pub healthy: bool,
    pub protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub users_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutBff {
    pub healthy: bool,
    pub protocol_version: String,
    pub user_count: u64,
    pub public_url: String,
}

struct AboutEnvConfig {
    configured: bool,
    ws_connector_status_url: String,
    stats_core_status_url: String,
    comms_protocol: String,
    comms_fixed_adapter: String,
    comms_version: Option<String>,
    comms_commit_hash: Option<String>,
    max_users: Option<u64>,
}

fn about_env() -> &'static AboutEnvConfig {
    static ENV: OnceLock<AboutEnvConfig> = OnceLock::new();
    ENV.get_or_init(|| {
        let ws_env = std::env::var("COMMS_WS_CONNECTOR_URL").ok().filter(|s| !s.is_empty());
        let stats_env = std::env::var("COMMS_STATS_URL").ok().filter(|s| !s.is_empty());
        let configured = ws_env.is_some() || stats_env.is_some();
        let ws_base = ws_env.unwrap_or_else(|| "http://127.0.0.1:5001".to_string());
        let stats_base = stats_env.unwrap_or_else(|| "http://127.0.0.1:5002".to_string());
        AboutEnvConfig {
            configured,
            ws_connector_status_url: format!("{}/status", ws_base.trim_end_matches('/')),
            stats_core_status_url: format!("{}/core-status", stats_base.trim_end_matches('/')),
            comms_protocol: std::env::var("COMMS_PROTOCOL")
                .unwrap_or_else(|_| "v3".to_string()),
            comms_fixed_adapter: std::env::var("COMMS_FIXED_ADAPTER")
                .unwrap_or_else(|_| "offline:offline".to_string()),
            comms_version: std::env::var("COMMS_VERSION").ok().filter(|s| !s.is_empty()),
            comms_commit_hash: std::env::var("COMMS_COMMIT_HASH")
                .ok()
                .filter(|s| !s.is_empty()),
            max_users: std::env::var("MAX_USERS")
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok()),
        }
    })
}

fn env_url(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn network_id(eth_network: &str) -> u64 {
    match eth_network {
        "mainnet" => 1,
        "sepolia" => 11155111,
        "goerli" => 5,
        "ropsten" => 3,
        "rinkeby" => 4,
        "kovan" => 42,
        _ => 1,
    }
}

#[derive(Clone, Copy)]
struct CommsProbe {
    healthy: bool,
    user_count: u64,
}

const COMMS_PROBE_TTL: Duration = Duration::from_secs(5);

const COMMS_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

fn comms_probe_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(COMMS_PROBE_TIMEOUT)
            .timeout(COMMS_PROBE_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

struct ProbeState {
    cached: Option<(Instant, CommsProbe)>,
}

static PROBE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

fn comms_probe_state() -> &'static AsyncMutex<ProbeState> {
    static STATE: OnceLock<AsyncMutex<ProbeState>> = OnceLock::new();
    STATE.get_or_init(|| AsyncMutex::new(ProbeState { cached: None }))
}

fn comms_probe_notify() -> &'static tokio::sync::Notify {
    static N: OnceLock<tokio::sync::Notify> = OnceLock::new();
    N.get_or_init(tokio::sync::Notify::new)
}

struct InFlightGuard {
    flag: &'static AtomicBool,
    notify: &'static tokio::sync::Notify,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
        self.notify.notify_waiters();
    }
}

async fn probe_comms() -> CommsProbe {
    loop {
        {
            let state = comms_probe_state().lock().await;
            if let Some((at, probe)) = state.cached {
                if at.elapsed() < COMMS_PROBE_TTL {
                    return probe;
                }
            }
        }
        if PROBE_IN_FLIGHT
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Acquire)
            .is_ok()
        {
            break;
        }
        comms_probe_notify().notified().await;
    }

    let guard = InFlightGuard {
        flag: &PROBE_IN_FLIGHT,
        notify: comms_probe_notify(),
    };

    let probe = run_probe().await;

    {
        let mut state = comms_probe_state().lock().await;
        state.cached = Some((Instant::now(), probe));
    }

    drop(guard);

    probe
}

async fn run_probe() -> CommsProbe {
    let client = comms_probe_client();
    let env = about_env();

    #[derive(serde::Deserialize)]
    struct CoreStatus {
        #[serde(default)]
        healthy: bool,
        #[serde(default, rename = "userCount")]
        user_count: u64,
    }

    let ws_fut = client.get(&env.ws_connector_status_url).send();
    let stats_fut = client.get(&env.stats_core_status_url).send();
    let (ws_resp, stats_resp) = tokio::join!(ws_fut, stats_fut);

    let ws_ok = matches!(
        ws_resp,
        Ok(resp) if resp.status() == reqwest::StatusCode::OK
    );

    let (stats_healthy, user_count) = match stats_resp {
        Ok(resp) if resp.status().is_success() => match resp.json::<CoreStatus>().await {
            Ok(cs) => (cs.healthy, cs.user_count),
            Err(_) => (false, 0),
        },
        _ => (false, 0),
    };

    CommsProbe {
        healthy: ws_ok && stats_healthy,
        user_count,
    }
}

fn build_comms_config(probe: CommsProbe) -> AboutComms {
    let env = about_env();
    AboutComms {
        healthy: probe.healthy,
        protocol: env.comms_protocol.clone(),
        version: env.comms_version.clone(),
        commit_hash: env.comms_commit_hash.clone(),
        users_count: Some(probe.user_count),
        adapter: if env.comms_fixed_adapter.is_empty() {
            None
        } else {
            Some(env.comms_fixed_adapter.clone())
        },
    }
}

fn content_is_healthy(sync_state: &str) -> bool {
    sync_state == "Syncing"
}

pub async fn get_about(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let sync_state = state.synchronization_state.get_state();
    let content_healthy = content_is_healthy(&sync_state);

    let mut healthy = content_healthy;
    let mut accepting_users = healthy;

    let comms = if about_env().configured {
        let comms_probe = probe_comms().await;
        let comms = build_comms_config(comms_probe);
        healthy = healthy && comms.healthy;
        let under_capacity = match about_env().max_users {
            Some(max) => comms_probe.user_count < max,
            None => true,
        };
        accepting_users = accepting_users && under_capacity;
        Some(comms)
    } else {
        None
    };

    let content_public_url = state.content_public_url.clone();
    let lambdas_public_url = state.lambdas_public_url.clone();

    let body = AboutResponse {
        healthy,
        content: AboutContent {
            healthy: content_healthy,
            version: Some(state.content_version.clone()),
            synchronization_status: sync_state,
            commit_hash: Some(state.commit_hash.clone()),
            public_url: content_public_url,
        },
        lambdas: AboutLambdas {
            healthy: true,
            version: Some(state.lambdas_version.clone()),
            commit_hash: Some(state.commit_hash.clone()),
            public_url: lambdas_public_url,
        },
        configurations: AboutConfigurations {
            network_id: network_id(&state.eth_network),
            global_scenes_urn: vec![],
            scenes_urn: vec![],
            realm_name: state.realm_name.clone(),
            map: Some(AboutConfigurationsMap {
                minimap_enabled: true,
                sizes: vec![
                    AboutMapSize { left: -150, top: 150, right: 150, bottom: -150 },
                    AboutMapSize { left: 62, top: 158, right: 162, bottom: 151 },
                    AboutMapSize { left: 151, top: 150, right: 163, bottom: 59 },
                ],
                satellite_view: Some(AboutMapView {
                    version: "v1".to_string(),
                    base_url: env_url(
                        "MAP_SATELLITE_BASE_URL",
                        "https://genesis.city/map/latest",
                    ),
                    suffix_url: ".jpg".to_string(),
                    top_left_offset: AboutMapOffset { x: -2, y: -6 },
                }),
                parcel_view: Some(AboutParcelView {
                    version: "v1".to_string(),
                    image_url: env_url(
                        "MAP_PARCEL_VIEW_URL",
                        "https://api.decentraland.org/v1/minimap.png",
                    ),
                }),
            }),
        },
        comms,

        bff: Some(AboutBff {
            healthy: true,
            protocol_version: "1.0_0".to_string(),
            user_count: 0,
            public_url: "/bff".to_string(),
        }),
        accepting_users,
    };

    let status = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_syncing_is_healthy() {
        assert!(content_is_healthy("Syncing"));
        assert!(!content_is_healthy("Bootstrapping"));
        assert!(!content_is_healthy("Partially synced"));
        assert!(!content_is_healthy("Failed"));
    }
}
