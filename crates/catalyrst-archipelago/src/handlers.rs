use crate::feed::IslandReport;
use crate::livekit::LivekitGrant;
use crate::peers::{to_parcel, Address, PeerState};
use crate::state::AppState;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
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
}

async fn ping() -> &'static str {
    "/ping"
}

#[derive(Serialize)]
struct ErrorResp {
    error: String,
}

#[derive(Serialize)]
struct OkErrorResp {
    ok: bool,
    error: String,
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
    Json(StatusResp {
        version: env!("CARGO_PKG_VERSION").to_string(),
        current_time: Utc::now().timestamp_millis(),
        commit_hash: s.cfg.commit_hash.clone(),
    })
}

/// The feed counters are appended, never reordered: the byte-exact shape assertions upstream
/// parity keeps are written against the first four fields.
#[derive(Serialize)]
struct HealthResp {
    healthy: bool,
    uptime_secs: i64,
    peers_total: usize,
    islands_total: usize,
    feed_connected: bool,
    feed_delivered: u64,
    feed_no_session_socket: u64,
    feed_deduplicated: u64,
    feed_undecodable: u64,
    feed_publish_dropped: u64,
}

async fn stats_health(State(s): State<AppState>) -> Json<HealthResp> {
    let uptime = Utc::now()
        .signed_duration_since(s.peers.started_at())
        .num_seconds()
        .max(0);
    Json(HealthResp {
        healthy: true,
        uptime_secs: uptime,
        peers_total: s.peers.peers_count(),
        islands_total: s.feed.islands_count(),
        feed_connected: s.publisher.is_connected(),
        feed_delivered: s.feed.delivered_count(),
        feed_no_session_socket: s.feed.no_session_socket_count(),
        feed_deduplicated: s.feed.deduplicated_count(),
        feed_undecodable: s.feed.undecodable_count(),
        feed_publish_dropped: s.publisher.dropped(),
    })
}

#[derive(Serialize)]
struct CoreStatusResp {
    healthy: bool,
    #[serde(rename = "userCount")]
    user_count: usize,
}

/// Reports what the clustering engine last announced on the discovery subject, not what this
/// replica happens to hold: the engine is a service of its own now, and a stale or missing
/// heartbeat is exactly the condition this endpoint exists to surface.
async fn core_status(State(s): State<AppState>) -> Json<CoreStatusResp> {
    Json(CoreStatusResp {
        healthy: s.feed.is_core_healthy(Utc::now().timestamp_millis()),
        user_count: s.feed.core_user_count() as usize,
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
    for p in s.peers.peers_snapshot().iter() {
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
        .peers
        .peers_snapshot()
        .iter()
        .filter(|p| {
            filter.is_empty() || filter.iter().any(|id| id.eq_ignore_ascii_case(&p.address))
        })
        .map(PeerResult::from)
        .collect();
    Json(PeersResp { ok: true, peers })
}

#[derive(Serialize)]
struct PeerResp {
    ok: bool,
    peer: Option<PeerResult>,
}

async fn peer_by_id(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match s.peers.peer(&id.to_lowercase()) {
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

/// A member this replica has never heard a heartbeat from is left out rather than invented: the
/// shape carries positions, and the replica holding that socket reports it with real ones.
fn process_island(island: &IslandReport, lookup: &HashMap<Address, PeerState>) -> IslandResult {
    let peers: Vec<PeerResult> = island
        .peers
        .iter()
        .filter_map(|addr| lookup.get(addr))
        .map(PeerResult::from)
        .collect();
    IslandResult {
        id: island.id.clone(),
        peers,
        max_peers: island.max_peers as usize,
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
    let lookup = s.peers.peers_by_address();
    let islands = s
        .feed
        .islands()
        .iter()
        .map(|i| process_island(i, &lookup))
        .collect();
    Json(IslandsResp { ok: true, islands })
}

async fn island_by_id(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match s.feed.island(&id) {
        Some(island) => {
            let lookup = s.peers.peers_by_address();
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
    for p in s.peers.peers_snapshot().iter() {
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
                parcels: scene
                    .parcels
                    .iter()
                    .map(|p| crate::content::parse_coord(p))
                    .collect(),
                thumbnail: scene.thumbnail,
                project_id: scene.project_id,
                creator: scene.creator,
                description: scene.description,
            }
        })
        .collect();
    hot.sort_by_key(|b| std::cmp::Reverse(b.users_total_count));
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

fn parse_json_body<T: for<'de> Deserialize<'de>>(
    body: &Bytes,
) -> Result<T, axum::response::Response> {
    serde_json::from_slice::<T>(body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResp {
                error: format!("invalid json body: {e}"),
            }),
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
        return (
            StatusCode::BAD_REQUEST,
            Json(OkErrorResp {
                ok: false,
                error: "missing address".into(),
            }),
        )
            .into_response();
    }
    if s.challenges.required() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(OkErrorResp {
                ok: false,
                error: "auth required; use /ws after /auth/challenge".into(),
            }),
        )
            .into_response();
    }
    s.peers.upsert_peer(
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
            Json(ErrorResp {
                error: "missing address".into(),
            }),
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
            Json(ErrorResp {
                error: e.to_string(),
            }),
        )
            .into_response();
    }
    if s.ban_checker.is_banned(&req.address).await {
        s.peers
            .kick_peer(&req.address.to_ascii_lowercase(), "banned");
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResp {
                error: "banned".into(),
            }),
        )
            .into_response();
    }
    if s.deny_list.is_denied(&req.address).await {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResp {
                error: "deny-listed".into(),
            }),
        )
            .into_response();
    }
    let addr = req.address.to_ascii_lowercase();
    let Some(island_id) = s.peers.island_of(&addr) else {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResp {
                error: "no authorized island".into(),
            }),
        )
            .into_response();
    };
    if !req.room.is_empty() && req.room != island_id {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResp {
                error: "room not authorized".into(),
            }),
        )
            .into_response();
    }
    let grant: LivekitGrant = s.livekit.mint(&addr, &island_id);
    Json(grant).into_response()
}
