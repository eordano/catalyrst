use crate::cluster::{to_parcel, Address, Island, PeerState};
use crate::gossip::GossipBatch;
use crate::livekit::LivekitGrant;
use crate::state::AppState;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use catalyrst_types::AuthChain;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/ping", get(ping))
        .merge(status_routes())
        .merge(api_routes())
}

pub fn status_routes() -> Router<AppState> {
    Router::new().route("/status", get(status))
}

pub fn api_routes() -> Router<AppState> {
    let mut router = Router::new();

    for prefix in ["", "/comms"] {
        router = router
            .route(&format!("{prefix}/parcels"), get(parcels))
            .route(&format!("{prefix}/peers"), get(peers))
            .route(&format!("{prefix}/peers/{{id}}"), get(peer_by_id))
            .route(&format!("{prefix}/islands"), get(islands))
            .route(&format!("{prefix}/islands/{{id}}"), get(island_by_id));
    }
    router
        .route("/hot-scenes", get(hot_scenes))
        .route("/core-status", get(core_status))
        .route("/stats/health", get(stats_health))
        .route("/heartbeat", post(heartbeat))
        .route("/auth/challenge", post(auth_challenge))
        .route("/auth/livekit-token", post(livekit_token))
        .route("/gossip/heartbeat", post(gossip_heartbeat))
        .route("/gossip/info", get(gossip_info))
}

async fn ping() -> &'static str {
    "/ping"
}

#[derive(Serialize)]
struct StatusResp {
    version: String,
    #[serde(rename = "currentTime")]
    current_time: i64,
    #[serde(rename = "commitHash")]
    commit_hash: String,
}

async fn status(State(s): State<AppState>) -> impl IntoResponse {
    let body = Json(StatusResp {
        version: env!("CARGO_PKG_VERSION").to_string(),
        current_time: Utc::now().timestamp_millis(),
        commit_hash: s.cfg.commit_hash.clone(),
    });
    ([(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")], body)
}

#[derive(Serialize)]
struct HealthResp {
    healthy: bool,
    uptime_secs: i64,
    peers_total: usize,
    islands_total: usize,
}

async fn stats_health(State(s): State<AppState>) -> Json<HealthResp> {
    let uptime = Utc::now()
        .signed_duration_since(s.cluster.started_at())
        .num_seconds()
        .max(0);
    Json(HealthResp {
        healthy: true,
        uptime_secs: uptime,
        peers_total: s.cluster.peers_count(),
        islands_total: s.cluster.islands_count(),
    })
}

#[derive(Serialize)]
struct CoreStatusResp {
    healthy: bool,
    #[serde(rename = "userCount")]
    user_count: usize,
}

async fn core_status(State(s): State<AppState>) -> Json<CoreStatusResp> {
    Json(CoreStatusResp {
        healthy: true,
        user_count: s.cluster.peers_count(),
    })
}

#[derive(Serialize)]
struct ParcelCoord {
    x: i32,
    y: i32,
}

#[derive(Serialize)]
struct ParcelResult {
    #[serde(rename = "peersCount")]
    peers_count: u32,
    parcel: ParcelCoord,
}

#[derive(Serialize)]
struct ParcelsResp {
    parcels: Vec<ParcelResult>,
}

async fn parcels(State(s): State<AppState>) -> Json<ParcelsResp> {
    let mut by_tile: HashMap<(i32, i32), u32> = HashMap::new();
    for p in s.cluster.peers_snapshot().iter() {
        let [px, _py, pz] = p.position;
        let [x, y] = to_parcel(px, pz);
        *by_tile.entry((x, y)).or_insert(0) += 1;
    }
    let parcels = by_tile
        .into_iter()
        .map(|((x, y), peers_count)| ParcelResult {
            peers_count,
            parcel: ParcelCoord { x, y },
        })
        .collect();
    Json(ParcelsResp { parcels })
}

#[derive(Serialize)]
struct PeerResult {
    id: Address,
    address: Address,
    #[serde(rename = "lastPing")]
    last_ping: i64,
    parcel: [i32; 2],
    position: [f32; 3],
}

impl From<&PeerState> for PeerResult {
    fn from(p: &PeerState) -> Self {
        let [px, _py, pz] = p.position;
        PeerResult {
            id: p.address.clone(),
            address: p.address.clone(),
            last_ping: p.last_heartbeat.timestamp_millis(),
            parcel: to_parcel(px, pz),
            position: p.position,
        }
    }
}

#[derive(Serialize)]
struct PeersResp {
    ok: bool,
    peers: Vec<PeerResult>,
}

async fn peers(State(s): State<AppState>, RawQuery(q): RawQuery) -> Json<PeersResp> {
    let filter = parse_id_filter(q.as_deref());
    let peers: Vec<PeerResult> = s
        .cluster
        .peers_snapshot()
        .iter()
        // Case-insensitive: addresses are stored lowercase but clients may pass
        // EIP-55 checksummed ids in ?id=.
        .filter(|p| filter.is_empty() || filter.iter().any(|id| id.eq_ignore_ascii_case(&p.address)))
        .map(PeerResult::from)
        .collect();
    Json(PeersResp { ok: true, peers })
}

#[derive(Serialize)]
struct PeerResp {
    ok: bool,
    peer: Option<PeerResult>,
}

async fn peer_by_id(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Peer addresses are stored lowercase (recovered signer); clients routinely
    // pass EIP-55 checksummed (mixed-case) addresses, which would 404 on an exact
    // match. Normalize the lookup key.
    match s.cluster.peer(&id.to_lowercase()) {
        Some(p) => (
            StatusCode::OK,
            Json(PeerResp {
                ok: true,
                peer: Some(PeerResult::from(&p)),
            }),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(PeerResp {
                ok: false,
                peer: None,
            }),
        ),
    }
}

#[derive(Serialize)]
struct IslandResult {
    id: String,
    peers: Vec<PeerResult>,
    #[serde(rename = "maxPeers")]
    max_peers: usize,
    center: [f32; 3],
    radius: f32,
}

fn process_island(island: &Island, lookup: &HashMap<Address, PeerState>) -> IslandResult {
    let peers: Vec<PeerResult> = island
        .peers
        .iter()
        .filter_map(|addr| lookup.get(addr))
        .map(PeerResult::from)
        .collect();
    IslandResult {
        id: island.id.clone(),
        peers,
        max_peers: island.max_peers,
        center: island.center,
        radius: island.radius,
    }
}

#[derive(Serialize)]
struct IslandsResp {
    ok: bool,
    islands: Vec<IslandResult>,
}

async fn islands(State(s): State<AppState>) -> Json<IslandsResp> {
    let lookup = s.cluster.peers_by_address();
    let islands = s
        .cluster
        .islands_snapshot()
        .iter()
        .map(|i| process_island(i, &lookup))
        .collect();
    Json(IslandsResp { ok: true, islands })
}

async fn island_by_id(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match s.cluster.island(&id) {
        Some(island) => {
            let lookup = s.cluster.peers_by_address();
            (StatusCode::OK, Json(process_island(&island, &lookup))).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Serialize)]
struct HotSceneInfo {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(rename = "baseCoords")]
    base_coords: [i32; 2],
    #[serde(rename = "usersTotalCount")]
    users_total_count: u32,
    parcels: Vec<[i32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumbnail: Option<String>,
    #[serde(rename = "projectId", skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

const HOT_SCENES_LIMIT: usize = 100;

async fn hot_scenes(State(s): State<AppState>) -> impl IntoResponse {
    let mut count_per_tile: HashMap<String, u32> = HashMap::new();
    for p in s.cluster.peers_snapshot().iter() {
        let [px, _py, pz] = p.position;
        let [x, y] = to_parcel(px, pz);
        *count_per_tile.entry(format!("{x},{y}")).or_insert(0) += 1;
    }
    let tiles: Vec<String> = count_per_tile.keys().cloned().collect();
    let scenes = match s.content.fetch_scenes(&tiles).await {
        Ok(scenes) => scenes,
        Err(_) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut hot: Vec<HotSceneInfo> = scenes
        .into_iter()
        .map(|scene| {
            let users_total_count: u32 = scene
                .parcels
                .iter()
                .map(|tile| count_per_tile.get(tile).copied().unwrap_or(0))
                .sum();
            HotSceneInfo {
                id: scene.id,
                name: scene.name,
                base_coords: scene.base,
                users_total_count,
                parcels: scene.parcels.iter().map(|p| crate::content::parse_coord(p)).collect(),
                thumbnail: scene.thumbnail,
                project_id: scene.project_id,
                creator: scene.creator,
                description: scene.description,
            }
        })
        .collect();
    hot.sort_by(|a, b| b.users_total_count.cmp(&a.users_total_count));
    hot.truncate(HOT_SCENES_LIMIT);
    Json(hot).into_response()
}

fn parse_id_filter(query: Option<&str>) -> Vec<String> {
    let Some(q) = query else { return Vec::new() };
    url::form_urlencoded::parse(q.as_bytes())
        .filter(|(k, _)| k == "id")
        .map(|(_, v)| v.into_owned())
        .collect()
}

fn parse_json_body<T: for<'de> Deserialize<'de>>(body: &Bytes) -> Result<T, axum::response::Response> {
    serde_json::from_slice::<T>(body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("invalid json body: {}", e)})),
        )
            .into_response()
    })
}

#[derive(Deserialize)]
pub struct HeartbeatReq {
    pub address: String,
    pub position: [f32; 3],
    pub parcel: [i32; 2],
    #[serde(default)]
    pub realm: Option<String>,
}

#[derive(Serialize)]
struct HeartbeatResp {
    ok: bool,
}

async fn heartbeat(State(s): State<AppState>, body: Bytes) -> impl IntoResponse {
    let req: HeartbeatReq = match parse_json_body(&body) {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    if req.address.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"ok": false, "error": "missing address"}))).into_response();
    }
    if s.challenges.required() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"ok": false, "error": "auth required; use /ws after /auth/challenge"})),
        )
            .into_response();
    }
    s.cluster.upsert_peer(
        req.address,
        req.position,
        req.parcel,
        req.realm.unwrap_or_else(|| "catalyrst".into()),
    );
    Json(HeartbeatResp { ok: true }).into_response()
}

#[derive(Deserialize)]
pub struct ChallengeReq {
    pub address: String,
}

#[derive(Serialize)]
struct ChallengeResp {
    challenge: String,
    address: String,
    ttl_secs: u64,
}

async fn auth_challenge(State(s): State<AppState>, body: Bytes) -> impl IntoResponse {
    let req: ChallengeReq = match parse_json_body(&body) {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    if req.address.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "missing address"})),
        )
            .into_response();
    }
    let challenge = s.challenges.issue(&req.address);
    Json(ChallengeResp {
        challenge,
        address: req.address,
        ttl_secs: s.cfg.auth.challenge_ttl_secs,
    })
    .into_response()
}

#[derive(Deserialize)]
pub struct LivekitTokenReq {
    pub address: String,
    pub challenge: String,
    pub auth_chain: AuthChain,
    pub room: String,
}

async fn livekit_token(State(s): State<AppState>, body: Bytes) -> impl IntoResponse {
    let req: LivekitTokenReq = match parse_json_body(&body) {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    if let Err(e) = s
        .challenges
        .redeem_and_verify(&req.address, &req.challenge, &req.auth_chain)
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    let grant: LivekitGrant = s.livekit.mint(&req.address, &req.room);
    Json(grant).into_response()
}

#[derive(Serialize)]
struct GossipInfoResp {
    node_id: String,
    armed: bool,
    peers: usize,
}

async fn gossip_info(State(s): State<AppState>) -> Json<GossipInfoResp> {
    Json(GossipInfoResp {
        node_id: s.gossip.node_id().to_string(),
        armed: s.gossip.is_armed(),
        peers: s.gossip.peers_count(),
    })
}

#[derive(Serialize)]
struct GossipApplyResp {
    ok: bool,
    applied: usize,
}

async fn gossip_heartbeat(
    State(s): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let from_node = match headers
        .get("X-Archipelago-Node")
        .and_then(|v| v.to_str().ok())
    {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "missing X-Archipelago-Node"})),
            )
                .into_response();
        }
    };
    let sig = match headers
        .get("X-Archipelago-Sig")
        .and_then(|v| v.to_str().ok())
    {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "missing X-Archipelago-Sig"})),
            )
                .into_response();
        }
    };
    let ts = match headers
        .get("X-Archipelago-Ts")
        .and_then(|v| v.to_str().ok())
    {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "missing X-Archipelago-Ts"})),
            )
                .into_response();
        }
    };
    if let Err(e) = s.gossip.verify(&body, &ts, &sig, &from_node) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    let batch: GossipBatch = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("bad json: {}", e)})),
            )
                .into_response();
        }
    };
    if batch.from_node != from_node {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "header/body node mismatch"})),
        )
            .into_response();
    }
    let applied = s.gossip.apply(&s.cluster, batch);
    Json(GossipApplyResp { ok: true, applied }).into_response()
}
