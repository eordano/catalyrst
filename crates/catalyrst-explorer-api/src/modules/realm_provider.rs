use crate::AppState;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

const CATALYST_STATUS_TTL: std::time::Duration = std::time::Duration::from_secs(3);
const HOT_SCENES_TTL: std::time::Duration = std::time::Duration::from_secs(3);

#[derive(Debug, Deserialize)]
pub struct AboutQuery {
    pub catalyst: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutResponse {
    pub healthy: bool,
    pub content: AboutContent,
    pub lambdas: AboutLambdas,
    pub configurations: AboutConfigurations,
    pub bff: AboutBff,
    pub accepting_users: bool,
    pub comms: AboutComms,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutContent {
    pub healthy: bool,
    pub version: String,
    pub synchronization_status: String,
    pub commit_hash: String,
    pub public_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutLambdas {
    pub healthy: bool,
    pub version: String,
    pub commit_hash: String,
    pub public_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutConfigurations {
    pub network_id: u64,
    pub global_scenes_urn: Vec<String>,
    pub scenes_urn: Vec<String>,
    pub realm_name: String,
    pub map: MapConfig,
    pub local_scene_parcels: Vec<String>,
    pub skybox: Skybox,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapConfig {
    pub minimap_enabled: bool,
    pub sizes: Vec<MapSize>,
    pub satellite_view: SatelliteView,
    pub parcel_view: ParcelView,
}

#[derive(Debug, Serialize)]
pub struct MapSize {
    pub left: i64,
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SatelliteView {
    pub version: String,
    pub base_url: String,
    pub suffix_url: String,
    pub top_left_offset: Offset,
}

#[derive(Debug, Serialize)]
pub struct Offset {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParcelView {
    pub version: String,
    pub image_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skybox {
    pub fixed_hour: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutBff {
    pub healthy: bool,
    pub protocol_version: String,
    pub user_count: u64,
    pub public_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutComms {
    pub version: String,
    pub commit_hash: String,
    pub healthy: bool,
    pub protocol: String,
    pub users_count: u64,
    pub adapter: String,
    pub fixed_adapter: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealmEntry {
    pub server_name: String,
    pub url: String,
    pub users_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotSceneInfo {
    pub id: String,
    pub name: String,
    pub base_coords: [i32; 2],
    pub parcels: Vec<[i32; 2]>,
    pub users_total_count: i64,
    pub realms: Vec<HotSceneRealm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotSceneRealm {
    pub server_name: String,
    pub users_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub version: String,
    pub current_time: i64,
    pub commit_hash: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/main", get(main_about))
        .route("/main/about", get(main_about))
        .route("/about", get(main_about))
        .route("/realms", get(realms))
        .route("/hot-scenes", get(hot_scenes))
        .route("/status", get(status))
}

pub(crate) struct CatalystStatus {
    version: String,
    commit_hash: String,
    sync_state: String,
}

// Reject any address that points back at this host or an internal network.
// A caller-supplied `?catalyst=` must never let a public request reach cloud
// IMDS (the cloud-metadata IMDS), loopback, or a private/tailnet service.
fn ip_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return ip_blocked(IpAddr::V4(mapped));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

// Validate an untrusted caller-supplied catalyst base URL before fetching it.
// Requires https and rejects any host that resolves to a blocked address.
// Returns the trimmed URL and the exact validated SocketAddr on success, or
// None to fall back to the trusted cfg. The returned addr is pinned by the
// caller so the later connect cannot re-resolve DNS to a different (internal)
// IP after this check (DNS-rebinding TOCTOU).
async fn validate_external_catalyst(base: &str) -> Option<(String, SocketAddr)> {
    let trimmed = base.trim_end_matches('/');
    let parsed = reqwest::Url::parse(trimmed).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let host = parsed.host_str()?;
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addr = if let Ok(ip) = host.parse::<IpAddr>() {
        if ip_blocked(ip) {
            return None;
        }
        SocketAddr::new(ip, port)
    } else {
        let addrs: Vec<_> = tokio::net::lookup_host((host, port)).await.ok()?.collect();
        if addrs.is_empty() || addrs.iter().any(|a| ip_blocked(a.ip())) {
            return None;
        }
        // Every resolved address passed the block-list; pin the first so the
        // connect uses exactly this validated IP with no second DNS lookup.
        addrs[0]
    };
    Some((trimmed.to_string(), addr))
}

// Shared builder settings: redirect-following disabled so a permitted external
// catalyst cannot 30x-bounce the fetch onto an internal target after the
// pre-flight host validation.
fn no_redirect_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent("catalyrst-explorer-api/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
}

fn no_redirect_client() -> &'static reqwest::Client {
    use std::sync::OnceLock;
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        no_redirect_builder()
            .build()
            .expect("failed to build no-redirect reqwest client")
    })
}

// Build a client that pins `host` to the exact `addr` that
// validate_external_catalyst approved, so the connection cannot re-resolve DNS
// to a rebound internal IP between validation and connect. reqwest ignores the
// port in the resolve override and uses the URL's port, so the URL's port is
// still honoured while the IP is locked to the validated one.
fn pinned_client(host: &str, addr: SocketAddr) -> Option<reqwest::Client> {
    no_redirect_builder().resolve(host, addr).build().ok()
}

// Cache the trusted configured-catalyst status for a short TTL so the HUD's
// constant /about polling collapses to one upstream fetch per window. Only the
// no-pin (trusted) path is cached; a validated external ?catalyst= is always
// fetched fresh through its pinned client, so the cache can never serve one
// caller's target to another (SSRF isolation preserved).
async fn fetch_catalyst_status_cached(state: &AppState, base: &str) -> Option<Arc<CatalystStatus>> {
    if let Some((at, cached)) = state.catalyst_status_cache.read().as_ref() {
        if at.elapsed() < CATALYST_STATUS_TTL {
            return cached.clone();
        }
    }
    let fetched = fetch_catalyst_status(base, None).await.map(Arc::new);
    *state.catalyst_status_cache.write() = Some((Instant::now(), fetched.clone()));
    fetched
}

async fn fetch_catalyst_status(base: &str, pin: Option<SocketAddr>) -> Option<CatalystStatus> {
    let base = base.trim_end_matches('/');
    // Untrusted (caller-supplied) bases carry a pinned addr: rebuild a client
    // that forces the connect onto the validated IP. The trusted-config path
    // has no pin and reuses the shared client.
    let client = match pin {
        Some(addr) => {
            let host = reqwest::Url::parse(base)
                .ok()?
                .host_str()
                .map(str::to_string)?;
            pinned_client(&host, addr)?
        }
        None => no_redirect_client().clone(),
    };
    let url = format!("{}/content/status", base);
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        let url = format!("{}/status", base);
        let resp = client.get(&url).send().await.ok()?;
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

async fn main_about(
    State(state): State<AppState>,
    Query(q): Query<AboutQuery>,
) -> Json<AboutResponse> {
    let cfg = &state.cfg;

    // The caller-supplied ?catalyst= is untrusted (SSRF): validate before use.
    // On rejection or absence, fall back to the operator-set (trusted) config.
    let validated = match q.catalyst.as_deref() {
        Some(candidate) => match validate_external_catalyst(candidate).await {
            Some(safe) => Some(safe),
            None => {
                tracing::warn!(
                    catalyst = %candidate,
                    "rejected caller-supplied catalyst URL (SSRF guard); using configured default"
                );
                None
            }
        },
        None => None,
    };

    let (base, lambdas_url, pin) = match validated {
        Some((safe, addr)) => {
            let lambdas = format!("{}/lambdas/", safe);
            (safe, lambdas, Some(addr))
        }
        None => match cfg.public_base_url.as_deref() {
            Some(public) => (public.to_string(), format!("{public}/lambdas/"), None),
            None => (
                cfg.catalyst_url.trim_end_matches('/').to_string(),
                format!("{}/", cfg.lambdas_url.trim_end_matches('/')),
                None,
            ),
        },
    };

    let content_url = format!("{}/content/", base);

    let realm_name = cfg.realm_name.clone();
    let comms_adapter = cfg.comms_adapter.clone();
    let comms_fixed_adapter = cfg.comms_fixed_adapter.clone();
    let network_id = cfg.network_id;
    let pkg_version = env!("CARGO_PKG_VERSION");
    let commit_hash = option_env!("GIT_COMMIT").unwrap_or("");

    // Only the trusted configured catalyst (no SSRF pin) is cached; a validated
    // external ?catalyst= is always fetched fresh through its pinned client.
    let catalyst = if pin.is_none() {
        fetch_catalyst_status_cached(&state, &base).await
    } else {
        fetch_catalyst_status(&base, pin).await.map(Arc::new)
    };
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

    let satellite_base_url = std::env::var("MAP_SATELLITE_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:5162/satellite".to_string());
    let parcel_image_url = std::env::var("MAP_PARCEL_VIEW_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:5162/v1/minimap.png".to_string());

    Json(AboutResponse {
        healthy: true,
        content: AboutContent {
            healthy: true,
            version: content_version.clone(),
            synchronization_status: sync_state,
            commit_hash: content_commit.clone(),
            public_url: content_url,
        },
        lambdas: AboutLambdas {
            healthy: true,
            version: content_version,
            commit_hash: content_commit,
            public_url: lambdas_url,
        },
        configurations: AboutConfigurations {
            network_id,
            global_scenes_urn: Vec::new(),
            scenes_urn: Vec::new(),
            realm_name,
            map: MapConfig {
                minimap_enabled: true,
                sizes: vec![
                    MapSize {
                        left: -150,
                        top: 150,
                        right: 150,
                        bottom: -150,
                    },
                    MapSize {
                        left: 62,
                        top: 158,
                        right: 162,
                        bottom: 151,
                    },
                    MapSize {
                        left: 151,
                        top: 150,
                        right: 163,
                        bottom: 59,
                    },
                ],
                satellite_view: SatelliteView {
                    version: "v1".to_string(),
                    base_url: satellite_base_url,
                    suffix_url: ".jpg".to_string(),
                    top_left_offset: Offset { x: -2, y: -6 },
                },
                parcel_view: ParcelView {
                    version: "v1".to_string(),
                    image_url: parcel_image_url,
                },
            },
            local_scene_parcels: Vec::new(),
            skybox: Skybox { fixed_hour: -1 },
        },
        bff: AboutBff {
            healthy: true,
            protocol_version: "1.0_0".to_string(),
            user_count: 0,
            public_url: cfg.bff_url.clone(),
        },
        accepting_users: true,
        comms: AboutComms {
            version: pkg_version.to_string(),
            commit_hash: commit_hash.to_string(),
            healthy: true,
            protocol: "v3".to_string(),
            users_count: 0,
            adapter: comms_adapter,
            fixed_adapter: comms_fixed_adapter,
        },
    })
}

async fn realms(State(state): State<AppState>) -> Json<Vec<RealmEntry>> {
    let cfg = &state.cfg;
    Json(vec![RealmEntry {
        server_name: cfg.realm_name.clone(),
        url: cfg.public_realm_url.clone(),
        users_count: 0,
    }])
}

async fn hot_scenes(State(state): State<AppState>) -> Json<Arc<Vec<HotSceneInfo>>> {
    if let Some((at, v)) = state.hot_scenes_cache.read().as_ref() {
        if at.elapsed() < HOT_SCENES_TTL {
            return Json(v.clone());
        }
    }
    let url = &state.cfg.hot_scenes_url;
    match state.http.get(url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<Vec<HotSceneInfo>>().await {
            Ok(scenes) => {
                let v = Arc::new(scenes);
                *state.hot_scenes_cache.write() = Some((Instant::now(), v.clone()));
                Json(v)
            }
            Err(err) => {
                tracing::warn!(%url, %err, "hot-scenes upstream body was not a conforming HotSceneInfo array; serving []");
                Json(Arc::new(Vec::new()))
            }
        },
        Ok(resp) => {
            tracing::warn!(%url, status = %resp.status(), "hot-scenes upstream error; serving []");
            Json(Arc::new(Vec::new()))
        }
        Err(err) => {
            tracing::warn!(%url, %err, "hot-scenes upstream unreachable; serving []");
            Json(Arc::new(Vec::new()))
        }
    }
}

async fn status() -> Json<StatusResponse> {
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        current_time: chrono::Utc::now().timestamp_millis(),
        commit_hash: option_env!("GIT_COMMIT").unwrap_or("").to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::future::IntoFuture;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

    async fn mock(
        route: &'static str,
        body: Value,
        hits: Arc<AtomicUsize>,
    ) -> std::net::SocketAddr {
        let app = axum::Router::new().route(
            route,
            axum::routing::get(move || {
                let h = hits.clone();
                let body = body.clone();
                async move {
                    h.fetch_add(1, SeqCst);
                    axum::Json(body)
                }
            }),
        );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(axum::serve(l, app).into_future());
        addr
    }

    #[tokio::test]
    async fn about_status_is_cached_per_ttl_on_trusted_path() {
        let (hits_a, hits_b) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let status_body = json!({
            "version": "9.9.9",
            "commitHash": "deadbeef",
            "synchronizationStatus": { "synchronizationState": "Bootstrapping" },
        });
        let addr_a = mock("/content/status", status_body.clone(), hits_a.clone()).await;
        let addr_b = mock("/content/status", status_body, hits_b.clone()).await;

        let mut cfg = crate::config::Config::from_env().unwrap();
        cfg.catalyst_url = format!("http://{addr_a}");
        let state = crate::build_state(&cfg).await.unwrap();

        for _ in 0..50 {
            let body = main_about(State(state.clone()), Query(AboutQuery { catalyst: None })).await;
            assert_eq!(body.0.content.synchronization_status, "Bootstrapping");
        }
        assert_eq!(
            hits_a.load(SeqCst),
            1,
            "cached path must collapse 50 polls into 1 fetch"
        );

        // SSRF guard + cache interaction: a caller-supplied ?catalyst= that the
        // guard rejects (non-https here; loopback is block-listed regardless)
        // must fall back to the trusted configured catalyst, served from cache --
        // so the attacker-named host is never fetched and no cache miss occurs.
        for _ in 0..2 {
            let _ = main_about(
                State(state.clone()),
                Query(AboutQuery {
                    catalyst: Some(format!("http://{addr_b}")),
                }),
            )
            .await;
        }
        assert_eq!(
            hits_b.load(SeqCst),
            0,
            "SSRF-rejected catalyst must never be fetched"
        );
        assert_eq!(
            hits_a.load(SeqCst),
            1,
            "rejected catalyst falls back to the cached configured status"
        );

        // TTL expiry, deterministic: rewind the cached timestamp instead of sleeping
        state.catalyst_status_cache.write().as_mut().unwrap().0 =
            std::time::Instant::now() - std::time::Duration::from_secs(60);
        let _ = main_about(State(state.clone()), Query(AboutQuery { catalyst: None })).await;
        assert_eq!(hits_a.load(SeqCst), 2);
    }

    #[tokio::test]
    async fn hot_scenes_cached_within_ttl_and_refreshed_after() {
        let hits = Arc::new(AtomicUsize::new(0));
        let scene = json!([{
            "id": "scene-1",
            "name": "scene-1",
            "baseCoords": [0, 0],
            "parcels": [[0, 0]],
            "usersTotalCount": 0,
            "realms": [],
        }]);
        let addr = mock("/hot-scenes", scene, hits.clone()).await;

        let mut cfg = crate::config::Config::from_env().unwrap();
        cfg.hot_scenes_url = format!("http://{addr}/hot-scenes");
        let state = crate::build_state(&cfg).await.unwrap();

        for _ in 0..50 {
            let body = hot_scenes(State(state.clone())).await;
            assert_eq!(body.0[0].name, "scene-1");
        }
        assert_eq!(
            hits.load(SeqCst),
            1,
            "50 polls inside the TTL must cost 1 upstream fetch"
        );

        state.hot_scenes_cache.write().as_mut().unwrap().0 =
            std::time::Instant::now() - std::time::Duration::from_secs(60);
        let body = hot_scenes(State(state.clone())).await;
        assert_eq!(body.0[0].name, "scene-1");
        assert_eq!(
            hits.load(SeqCst),
            2,
            "stale cache must trigger exactly one refetch"
        );
    }

    #[tokio::test]
    async fn about_doc_advertises_the_public_base_never_the_internal_reach() {
        let hits = Arc::new(AtomicUsize::new(0));
        let status_body = json!({
            "version": "9.9.9",
            "commitHash": "deadbeef",
            "synchronizationStatus": { "synchronizationState": "Syncing" },
        });
        let addr = mock("/content/status", status_body, hits.clone()).await;

        let mut cfg = crate::config::Config::from_env().unwrap();
        cfg.catalyst_url = format!("http://{addr}");
        cfg.public_base_url = Some("https://interconnected.example".into());
        let state = crate::build_state(&cfg).await.unwrap();
        let body = main_about(State(state.clone()), Query(AboutQuery { catalyst: None })).await;
        assert_eq!(
            body.0.content.public_url,
            "https://interconnected.example/content/"
        );
        assert_eq!(
            body.0.lambdas.public_url,
            "https://interconnected.example/lambdas/"
        );

        let mut cfg = crate::config::Config::from_env().unwrap();
        cfg.catalyst_url = format!("http://{addr}");
        cfg.lambdas_url = format!("http://{addr}/lambdas");
        cfg.public_base_url = None;
        let state = crate::build_state(&cfg).await.unwrap();
        let body = main_about(State(state.clone()), Query(AboutQuery { catalyst: None })).await;
        assert_eq!(body.0.content.public_url, format!("http://{addr}/content/"));
    }
}
