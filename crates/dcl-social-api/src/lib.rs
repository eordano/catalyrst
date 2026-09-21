//! Standalone social storage and client-signed Foundation relay. No wallet keys or login flow.
mod community_assets;
mod discovery;
mod event_creation;
fn default_visibility() -> String {
    "all".into()
}
pub mod config;
mod conversation;
mod images;
mod location;
mod management;
mod security;
pub mod telemetry;
use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use catalyrst_crypto::{
    build_payload_v6, default_eip1654_validator, verify::verify_auth_chain_async, AuthChain,
    AuthLinkType,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
type ApiResult<T> = Result<T, ApiError>;
#[derive(Clone, Debug)]
pub struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}
impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        tracing::error!("social database: {e}");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Storage unavailable".into(),
        )
    }
}
fn bad(s: &str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, s.into())
}
/// How long a read capability may replay its signature: inside Foundation's 60 second window.
const READ_WINDOW_MS: i64 = 50_000;
fn denied(s: &str) -> ApiError {
    ApiError(StatusCode::FORBIDDEN, s.into())
}
fn unavailable() -> ApiError {
    ApiError(
        StatusCode::BAD_GATEWAY,
        "Foundation unavailable; try again".into(),
    )
}

pub struct Store {
    db: Mutex<Connection>,
    upstream: String,
    client: reqwest::Client,
    locations: tokio::sync::Mutex<(i64, Value)>,
    swept: AtomicI64,
    memo: Memo,
}
impl Store {
    pub fn open(path: &str, upstream: &str) -> anyhow::Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS actions(id TEXT PRIMARY KEY, wallet TEXT NOT NULL, operation TEXT NOT NULL, prepared TEXT NOT NULL, expires INTEGER NOT NULL, state TEXT NOT NULL DEFAULT 'pending');
            CREATE INDEX IF NOT EXISTS actions_expiry ON actions(expires);
            CREATE TABLE IF NOT EXISTS messages(seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, community TEXT NOT NULL, wallet TEXT NOT NULL, text TEXT NOT NULL, scene TEXT, created INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS message_history ON messages(community,seq);")?;
        conversation::migrate(&db)?;
        Ok(Self {
            db: Mutex::new(db),
            upstream: upstream.into(),
            locations: tokio::sync::Mutex::new((0, Value::Null)),
            swept: AtomicI64::new(0),
            memo: Memo::default(),
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(12))
                .build()?,
        })
    }
}

type MemoSlot = Arc<tokio::sync::Mutex<(i64, Option<ApiResult<Value>>)>>;
/// Single-flight TTL memo for proxied reads; failures are held 10 s like the location snapshot.
#[derive(Default)]
struct Memo(Mutex<HashMap<String, MemoSlot>>);
impl Memo {
    fn slot(&self, key: &str) -> MemoSlot {
        let mut slots = self.0.lock().unwrap();
        if slots.len() >= 4096 {
            let at = now();
            slots.retain(|_, slot| !slot.try_lock().is_ok_and(|s| s.0 <= at));
        }
        slots.entry(key.to_owned()).or_default().clone()
    }
    fn forget(&self, prefix: &str) {
        self.0
            .lock()
            .unwrap()
            .retain(|key, _| !key.starts_with(prefix));
    }
}
async fn memoized<F, Fut>(s: &Store, key: String, ttl_ms: i64, fetch: F) -> ApiResult<Value>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ApiResult<Value>>,
{
    let slot = s.memo.slot(&key);
    let mut guard = slot.lock().await;
    if let (expires, Some(cached)) = &*guard {
        if *expires > now() {
            return cached.clone();
        }
    }
    let result = fetch().await;
    let ttl = if result.is_ok() { ttl_ms } else { 10_000 };
    *guard = (now() + ttl, Some(result.clone()));
    result
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    CreateCommunity {
        name: String,
        description: String,
        privacy: String,
        #[serde(default = "default_visibility")]
        visibility: String,
        #[serde(default)]
        thumbnail: Option<String>,
    },
    UpdateCommunity {
        community_id: String,
        name: String,
        description: String,
        privacy: String,
        #[serde(default = "default_visibility")]
        visibility: String,
        #[serde(default)]
        thumbnail: Option<String>,
    },
    ActiveCommunityVoiceChats,
    CommunityPlaces {
        community_id: String,
        #[serde(default)]
        offset: u32,
    },
    AddCommunityPlaces {
        community_id: String,
        place_ids: Vec<String>,
    },
    RemoveCommunityPlace {
        community_id: String,
        place_id: String,
    },
    CreateEvent {
        event: event_creation::EventDraft,
    },
    MyEvents,
    EventAttendees {
        event_id: String,
    },
    EventRsvp {
        event_id: String,
        attending: bool,
    },
    MyCommunities,
    MyJoinRequests {
        #[serde(default)]
        offset: u32,
    },
    MyCommunityInvitations {
        #[serde(default)]
        offset: u32,
    },
    CancelCommunityRequest {
        community_id: String,
        request_id: String,
    },
    PrivateChatToken,
    OwnLocation,
    ManageCommunity {
        community_id: String,
        action: management::Action,
    },
    CommunityRequests {
        community_id: String,
        offset: u32,
    },
    ResolveRequest {
        community_id: String,
        request_id: String,
        accept: bool,
    },
    CommunityMembers {
        community_id: String,
        offset: u32,
    },
    CommunityReports {
        community_id: String,
    },
    MessageAction {
        community_id: String,
        message_id: String,
        action: conversation::Action,
    },
    Reply {
        community_id: String,
        message_id: String,
        text: String,
    },
    CommunityDetails {
        community_id: String,
    },
    JoinCommunity {
        community_id: String,
    },
    RequestCommunityJoin {
        community_id: String,
    },
    CommunityPosts {
        community_id: String,
    },
    PublishPost {
        community_id: String,
        content: String,
    },
    ConfigureChannel {
        community_id: String,
        channel: String,
        config: conversation::ChannelConfig,
    },
    CreateChannel {
        community_id: String,
        name: String,
        private: bool,
    },
    OpenCommunity {
        community_id: String,
        #[serde(default = "general_channel")]
        channel: String,
    },
    SendMessage {
        community_id: String,
        #[serde(default = "general_channel")]
        channel: String,
        text: String,
        scene: Option<Scene>,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub x: i16,
    pub y: i16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
}
fn general_channel() -> String {
    "general".into()
}
impl Operation {
    fn channel(&self) -> &str {
        match self {
            Self::ConfigureChannel { channel, .. }
            | Self::OpenCommunity { channel, .. }
            | Self::SendMessage { channel, .. } => channel,
            _ => "general",
        }
    }
    fn community(&self) -> Option<&str> {
        match self {
            Self::CreateCommunity { .. }
            | Self::ActiveCommunityVoiceChats
            | Self::CreateEvent { .. }
            | Self::MyEvents
            | Self::EventAttendees { .. }
            | Self::EventRsvp { .. }
            | Self::MyCommunities
            | Self::MyJoinRequests { .. }
            | Self::MyCommunityInvitations { .. }
            | Self::PrivateChatToken
            | Self::OwnLocation => None,
            Self::CommunityPlaces { community_id, .. }
            | Self::AddCommunityPlaces { community_id, .. }
            | Self::RemoveCommunityPlace { community_id, .. }
            | Self::ManageCommunity { community_id, .. }
            | Self::CommunityRequests { community_id, .. }
            | Self::ResolveRequest { community_id, .. }
            | Self::CancelCommunityRequest { community_id, .. }
            | Self::UpdateCommunity { community_id, .. }
            | Self::CommunityMembers { community_id, .. }
            | Self::CommunityReports { community_id }
            | Self::MessageAction { community_id, .. }
            | Self::Reply { community_id, .. }
            | Self::CommunityDetails { community_id }
            | Self::JoinCommunity { community_id }
            | Self::RequestCommunityJoin { community_id }
            | Self::ConfigureChannel { community_id, .. }
            | Self::CreateChannel { community_id, .. }
            | Self::OpenCommunity { community_id, .. }
            | Self::CommunityPosts { community_id }
            | Self::PublishPost { community_id, .. }
            | Self::SendMessage { community_id, .. } => Some(community_id),
        }
    }
    fn validate(&self) -> ApiResult<()> {
        if let Some(id) = self.community() {
            Uuid::parse_str(id).map_err(|_| bad("Invalid community ID"))?;
        }
        if let Self::ConfigureChannel {
            config, channel, ..
        } = self
        {
            config.validate()?;
            if matches!(channel.as_str(), "announcements" | "voice") {
                return Err(bad("Use Foundation settings for this channel"));
            }
        }
        if let Self::ManageCommunity { action, .. } = self {
            action.validate()?;
        }
        if let Self::CreateChannel { name, .. } = self {
            if !conversation::valid_channel(name)
                || matches!(
                    name.as_str(),
                    "general" | "announcements" | "voice" | "about" | "hangouts" | "members"
                )
            {
                return Err(bad(
                    "Use 1\u{2013}32 lowercase letters, numbers or hyphens for a channel name",
                ));
            }
        }
        if !conversation::valid_channel(self.channel()) {
            return Err(bad("Invalid channel"));
        }
        if let Self::CreateCommunity {
            name,
            description,
            privacy,
            visibility,
            ..
        }
        | Self::UpdateCommunity {
            name,
            description,
            privacy,
            visibility,
            ..
        } = self
        {
            if name.trim().is_empty()
                || name.chars().count() > 30
                || description.trim().is_empty()
                || description.chars().count() > 500
                || !matches!(privacy.as_str(), "public" | "private")
                || !matches!(visibility.as_str(), "all" | "unlisted")
            {
                return Err(bad("Provide a name (up to 30 characters), description (up to 500), and valid privacy"));
            }
        }
        if let Self::CreateCommunity {
            thumbnail: Some(thumbnail),
            ..
        }
        | Self::UpdateCommunity {
            thumbnail: Some(thumbnail),
            ..
        } = self
        {
            community_assets::validate_thumbnail(thumbnail)?;
        }
        if let Self::AddCommunityPlaces { place_ids, .. } = self {
            if place_ids.is_empty()
                || place_ids.len() > 100
                || place_ids
                    .iter()
                    .any(|id| !community_assets::valid_place_id(id))
            {
                return Err(bad("Choose 1\u{2013}100 valid place IDs"));
            }
        }
        if let Self::RemoveCommunityPlace { place_id, .. } = self {
            if !community_assets::valid_place_id(place_id) {
                return Err(bad("Invalid place ID"));
            }
        }
        if let Self::CreateEvent { event } = self {
            event.validate()?;
        }
        match self {
            Self::EventAttendees { event_id } | Self::EventRsvp { event_id, .. } => {
                Uuid::parse_str(event_id).map_err(|_| bad("Invalid event ID"))?;
            }
            Self::ResolveRequest { request_id, .. }
            | Self::CancelCommunityRequest { request_id, .. } => {
                Uuid::parse_str(request_id).map_err(|_| bad("Invalid request ID"))?;
            }
            Self::CommunityPlaces { offset, .. }
            | Self::CommunityRequests { offset, .. }
            | Self::CommunityMembers { offset, .. }
            | Self::MyJoinRequests { offset }
            | Self::MyCommunityInvitations { offset }
                if *offset > 100_000 =>
            {
                return Err(bad("Invalid member page"))
            }
            Self::MessageAction { message_id, .. } | Self::Reply { message_id, .. } => {
                Uuid::parse_str(message_id).map_err(|_| bad("Invalid message ID"))?;
            }
            _ => {}
        }
        if let Self::Reply { text, .. } = self {
            if text.trim().is_empty() || text.chars().count() > 4000 {
                return Err(bad("Replies need 1\u{2013}4,000 characters"));
            }
        }
        if let Self::PublishPost { content, .. } = self {
            if content.trim().is_empty() || content.chars().count() > 1000 {
                return Err(bad("Announcements need 1\u{2013}1,000 characters"));
            }
        }
        if let Self::SendMessage { text, scene, .. } = self {
            if text.chars().count() > 4000 || (text.trim().is_empty() && scene.is_none()) {
                return Err(bad(
                    "Write a message (up to 4,000 characters) or share a scene",
                ));
            }
            if scene.as_ref().is_some_and(|s| match &s.world {
                Some(world) => !discovery::valid_world(world),
                None => !(-150..=150).contains(&s.x) || !(-150..=150).contains(&s.y),
            }) {
                return Err(bad("Coordinates must be between -150 and 150"));
            }
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prepare {
    wallet: String,
    operation: Operation,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    id: String,
    wallet: String,
    operation: Operation,
    url: String,
    method: String,
    body: Option<String>,
    timestamp: String,
    metadata: String,
    payload: String,
    expires_at: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Complete {
    auth_chain: AuthChain,
}

pub fn app(state: Arc<Store>, assets: PathBuf) -> Router {
    Router::new()
        .route("/api/health", get(|| async { Json(json!({"service":"dcl.social","capabilities":["signed-foundation-reads","durable-community-chat","scene-sharing","channels","threads","reactions","pins","community-management"]})) }))
        .route("/api/communities", get(discover))
        .route("/api/profiles/{wallet}", get(profile))
        .route("/api/places", get(places))
        .route("/api/place/{id}", get(community_assets::place))
        .route("/api/events", get(discovery::events))
        .route("/api/events/{id}", get(discovery::event))
        .route("/api/events/{id}/attendees", get(discovery::attendees))
        .route("/api/worlds", get(discovery::worlds))
        .route("/api/locations", get(location::friends))
        .route("/api/image", get(images::get_image))
        .route("/api/telemetry", post(telemetry::client_error))
        .route("/api/actions", post(prepare).layer(DefaultBodyLimit::max(768 * 1024)))
        .route("/api/actions/{id}/complete", post(complete))
        .route("/api/communities/{id}/messages", get(history))
        .route("/api/{*path}", get(|| async { (StatusCode::NOT_FOUND, Json(json!({"error":"Unknown API route"}))) }))
        .fallback_service(ServeDir::new(&assets).not_found_service(ServeFile::new(assets.join("index.html"))))
        .layer(axum::middleware::from_fn(|request: axum::extract::Request, next: axum::middleware::Next| async move {
            let route = request.extensions().get::<axum::extract::MatchedPath>().map(|p| p.as_str().to_string()).unwrap_or_default();
            let method = request.method().to_string();
            let mut response = next.run(request).await;
            if response.status().is_server_error() && route != "/api/telemetry" {
                telemetry::emit("server", "http", &format!("{method} {route}: {}", response.status()), "");
            }
            response.headers_mut().entry("cache-control").or_insert("no-store".parse().unwrap());
            security::headers(response.headers_mut());
            response
        }))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(state)
}
async fn bounded_json(mut response: reqwest::Response) -> ApiResult<Value> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| unavailable())
}
#[derive(Deserialize)]
struct DiscoverQuery {
    #[serde(default)]
    search: String,
    #[serde(default)]
    offset: u32,
}
async fn discover(
    State(s): State<Arc<Store>>,
    Query(q): Query<DiscoverQuery>,
) -> ApiResult<Json<Value>> {
    if q.search.len() > 200 || q.offset > 100_000 {
        return Err(bad("Invalid community search"));
    }
    let key = format!("communities:{}:{}", q.offset, q.search);
    let value = memoized(&s, key, 30_000, || async {
        let r = s
            .client
            .get(format!("{}/v1/communities", s.upstream))
            .query(&[
                ("limit", "100"),
                ("offset", &q.offset.to_string()),
                ("search", &q.search),
            ])
            .send()
            .await
            .map_err(|_| unavailable())?;
        if !r.status().is_success() {
            return Err(unavailable());
        }
        bounded_json(r).await
    })
    .await?;
    Ok(Json(value))
}
async fn prepare(
    State(s): State<Arc<Store>>,
    Json(input): Json<Prepare>,
) -> ApiResult<Json<Prepared>> {
    input.operation.validate()?;
    let wallet = input.wallet.to_lowercase();
    if wallet.len() != 42
        || !wallet.starts_with("0x")
        || !wallet[2..].bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(bad("Invalid wallet"));
    }
    let id = Uuid::new_v4().to_string();
    let timestamp = now().to_string();
    let mut path = input
        .operation
        .community()
        .map(|id| format!("/v1/communities/{id}"))
        .unwrap_or("/v1/communities".into());
    if matches!(
        input.operation,
        Operation::MyJoinRequests { .. } | Operation::MyCommunityInvitations { .. }
    ) {
        path = format!("/v1/members/{wallet}/requests");
    }
    if matches!(input.operation, Operation::PrivateChatToken) {
        path = "/private-messages/token".into();
    }
    if matches!(input.operation, Operation::OwnLocation) {
        path = format!("/peers/{wallet}");
    }
    let (method, body) = match &input.operation {
        Operation::CreateCommunity {
            name,
            description,
            privacy,
            visibility,
            ..
        }
        | Operation::UpdateCommunity {
            name,
            description,
            privacy,
            visibility,
            ..
        } => {
            let boundary = format!("dcl-social-{id}");
            let body = [("name",name),("description",description),("privacy",privacy),("visibility",visibility)].into_iter().map(|(key,value)| format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{key}\"\r\n\r\n{value}\r\n")).collect::<String>() + &format!("--{boundary}--\r\n");
            (
                if matches!(input.operation, Operation::CreateCommunity { .. }) {
                    "POST"
                } else {
                    "PUT"
                },
                Some(body),
            )
        }
        Operation::ActiveCommunityVoiceChats => {
            path = "/v1/community-voice-chats/active".into();
            ("GET", None)
        }
        Operation::CommunityPlaces { .. } => {
            path.push_str("/places");
            ("GET", None)
        }
        Operation::AddCommunityPlaces { place_ids, .. } => {
            path.push_str("/places");
            ("POST", Some(json!({"placeIds":place_ids}).to_string()))
        }
        Operation::RemoveCommunityPlace { place_id, .. } => {
            path.push_str(&format!("/places/{place_id}"));
            ("DELETE", None)
        }
        Operation::CreateEvent { event } => {
            path = "/api/events".into();
            ("POST", Some(event.body().to_string()))
        }
        Operation::MyEvents => {
            path = "/api/events/attending".into();
            ("GET", None)
        }
        Operation::EventAttendees { event_id } => {
            path = format!("/api/events/{event_id}/attendees");
            ("GET", None)
        }
        Operation::EventRsvp {
            event_id,
            attending,
        } => {
            path = format!("/api/events/{event_id}/attendees");
            (if *attending { "POST" } else { "DELETE" }, None)
        }
        Operation::ManageCommunity { action, .. } => action.request(&mut path, &wallet),
        Operation::ResolveRequest {
            request_id, accept, ..
        } => {
            path.push_str(&format!("/requests/{request_id}"));
            (
                "PATCH",
                Some(json!({"intention":if *accept {"accepted"} else {"rejected"}}).to_string()),
            )
        }
        Operation::CancelCommunityRequest { request_id, .. } => {
            path.push_str(&format!("/requests/{request_id}"));
            ("PATCH", Some(json!({"intention":"cancelled"}).to_string()))
        }
        Operation::CommunityRequests { .. } => {
            path.push_str("/requests");
            ("GET", None)
        }
        Operation::CommunityMembers { .. } => {
            path.push_str("/members");
            ("GET", None)
        }
        Operation::JoinCommunity { .. } => {
            path.push_str("/members");
            ("POST", None)
        }
        Operation::RequestCommunityJoin { .. } => {
            path.push_str("/requests");
            (
                "POST",
                Some(json!({"targetedAddress":wallet,"type":"request_to_join"}).to_string()),
            )
        }
        Operation::PublishPost { content, .. } => {
            path.push_str("/posts");
            ("POST", Some(json!({"content":content}).to_string()))
        }
        Operation::CommunityPosts { .. } => {
            path.push_str("/posts");
            ("GET", None)
        }
        _ => ("GET", None),
    };
    let member_query;
    let query = if let Operation::ManageCommunity {
        action: management::Action::Bans { offset },
        ..
    } = &input.operation
    {
        member_query = format!("?limit=100&offset={offset}");
        member_query.as_str()
    } else if let Operation::CommunityRequests { offset, .. } = &input.operation {
        member_query = format!("?type=request_to_join&limit=100&offset={offset}");
        member_query.as_str()
    } else if let Operation::CommunityMembers { offset, .. }
    | Operation::CommunityPlaces { offset, .. } = &input.operation
    {
        member_query = format!("?limit=100&offset={offset}");
        member_query.as_str()
    } else if let Operation::MyJoinRequests { offset }
    | Operation::MyCommunityInvitations { offset } = &input.operation
    {
        let kind = if matches!(input.operation, Operation::MyCommunityInvitations { .. }) {
            "invite"
        } else {
            "request_to_join"
        };
        member_query = format!("?type={kind}&limit=100&offset={offset}");
        member_query.as_str()
    } else if matches!(input.operation, Operation::MyCommunities) {
        "?onlyMemberOf=true&limit=100"
    } else {
        ""
    };
    let origin = if matches!(
        input.operation,
        Operation::CreateEvent { .. }
            | Operation::MyEvents
            | Operation::EventAttendees { .. }
            | Operation::EventRsvp { .. }
    ) {
        "https://events.decentraland.org"
    } else if matches!(input.operation, Operation::PrivateChatToken) {
        "https://comms-gatekeeper.decentraland.org"
    } else if matches!(input.operation, Operation::OwnLocation) {
        "https://archipelago-ea-stats.decentraland.org"
    } else {
        &s.upstream
    };
    let url = format!("{origin}{path}{query}");
    // Legacy Signed Fetch does not bind the body or query. Bind the complete local
    // intent and destination in lowercase metadata, compatible with both formats.
    let digest: String = Sha256::digest(
        serde_json::to_vec(&json!({"wallet":wallet,"url":url,"operation":input.operation}))
            .unwrap(),
    )
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect();
    let signer = if matches!(input.operation, Operation::PrivateChatToken) {
        "dcl:explorer"
    } else {
        "dcl.social"
    };
    let metadata = json!({"signer":signer,"intent":digest,"request":id}).to_string();
    let prepared = Prepared {
        id: id.clone(),
        wallet: wallet.clone(),
        operation: input.operation,
        url,
        method: method.into(),
        body,
        payload: build_payload_v6(method, &path, &timestamp, &metadata),
        timestamp,
        metadata,
        expires_at: now() + 90_000,
    };
    let db = s.db.lock().unwrap();
    let at = now();
    if at - s.swept.load(Ordering::Relaxed) >= 60_000 {
        s.swept.store(at, Ordering::Relaxed);
        db.execute("DELETE FROM actions WHERE expires < ?", [at - 300_000])?;
    }
    let full: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM actions LIMIT 1 OFFSET 9999)",
        [],
        |r| r.get(0),
    )?;
    if full {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many pending actions".into(),
        ));
    }
    db.execute(
        "INSERT INTO actions(id,wallet,operation,prepared,expires) VALUES (?,?,?,?,?)",
        params![
            id,
            wallet,
            serde_json::to_string(&prepared.operation).unwrap(),
            serde_json::to_string(&prepared).unwrap(),
            prepared.expires_at
        ],
    )?;
    Ok(Json(prepared))
}
fn load(s: &Store, id: &str) -> ApiResult<(Prepared, String)> {
    let row: Option<(String, String)> =
        s.db.lock()
            .unwrap()
            .query_row("SELECT prepared,state FROM actions WHERE id=?", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
    let (data, status) =
        row.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Action not found".into()))?;
    let p: Prepared = serde_json::from_str(&data).map_err(|_| bad("Invalid stored action"))?;
    if p.expires_at <= now() {
        return Err(ApiError(
            StatusCode::GONE,
            "Signature expired; prepare again".into(),
        ));
    }
    Ok((p, status))
}
async fn verify(p: &Prepared, chain: &AuthChain) -> ApiResult<()> {
    if chain.len() < 2
        || chain.first().is_none_or(|l| {
            l.link_type != AuthLinkType::SIGNER || !l.payload.eq_ignore_ascii_case(&p.wallet)
        })
    {
        return Err(denied("Signature belongs to another wallet"));
    }
    verify_auth_chain_async(
        chain,
        &p.payload,
        Some(now()),
        default_eip1654_validator().map(|v| v.as_ref()),
    )
    .await
    .map_err(|_| denied("Invalid or expired signature"))
}
async fn upstream(
    s: &Store,
    p: &Prepared,
    chain: &AuthChain,
) -> ApiResult<(Value, Option<conversation::ChannelConfig>)> {
    let mut request = s
        .client
        .request(
            reqwest::Method::from_bytes(p.method.as_bytes()).map_err(|_| bad("Invalid method"))?,
            &p.url,
        )
        .header("x-identity-timestamp", &p.timestamp)
        .header("x-identity-metadata", &p.metadata);
    if let Some(body) = &p.body {
        request = request
            .header(
                "content-type",
                if matches!(
                    p.operation,
                    Operation::CreateCommunity { .. } | Operation::UpdateCommunity { .. }
                ) {
                    format!("multipart/form-data; boundary=dcl-social-{}", p.id)
                } else {
                    "application/json".into()
                },
            )
            .body(match &p.operation {
                Operation::CreateCommunity { thumbnail, .. }
                | Operation::UpdateCommunity { thumbnail, .. } => {
                    community_assets::multipart(body, &p.id, thumbnail.as_deref())?
                }
                _ => body.as_bytes().to_vec(),
            });
    }
    for (i, link) in chain.iter().enumerate() {
        request = request.header(
            format!("x-identity-auth-chain-{i}"),
            serde_json::to_string(link).unwrap(),
        );
    }
    let response = request.send().await.map_err(|_| unavailable())?;
    if !response.status().is_success() {
        return Err(if matches!(response.status().as_u16(), 401 | 403 | 404) {
            denied("Foundation denied this request")
        } else if response.status().is_client_error() {
            ApiError(response.status(), "Foundation could not complete this action. Refresh its status before trying again.".into())
        } else {
            unavailable()
        });
    }
    if response.status() == StatusCode::NO_CONTENT {
        return Ok((json!({"data":{}}), None));
    }
    let data = bounded_json(response).await?;
    if let Operation::MessageAction {
        community_id: id, ..
    }
    | Operation::Reply {
        community_id: id, ..
    }
    | Operation::ConfigureChannel {
        community_id: id, ..
    }
    | Operation::CommunityReports { community_id: id }
    | Operation::CreateChannel {
        community_id: id, ..
    }
    | Operation::OpenCommunity {
        community_id: id, ..
    }
    | Operation::SendMessage {
        community_id: id, ..
    } = &p.operation
    {
        // Foundation honours a signed request for 60 seconds and afterwards answers a public
        // community as if nobody had signed: no role at all. That is a spent signature, not a
        // member who left, so ask for a fresh one instead of reporting a lost membership.
        if data["data"]["id"].as_str() == Some(id) && data["data"].get("role").is_none() {
            return Err(ApiError(
                StatusCode::GONE,
                "Refresh this community to continue".into(),
            ));
        }
        if data["data"]["id"].as_str() != Some(id)
            || data["data"]["active"] != true
            || !matches!(
                data["data"]["role"].as_str(),
                Some("owner" | "moderator" | "member")
            )
        {
            return Err(denied(
                "Join this community in Decentraland to open its chat",
            ));
        }
    }
    let config = conversation::authorize(
        s,
        &p.operation,
        data["data"]["role"].as_str().unwrap_or(""),
        &p.wallet,
    )?;
    Ok((data, config))
}
async fn complete(
    State(s): State<Arc<Store>>,
    Path(id): Path<String>,
    Json(input): Json<Complete>,
) -> ApiResult<Json<Value>> {
    let (p, status) = load(&s, &id)?;
    verify(&p, &input.auth_chain).await?;
    if status != "pending" {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Action already submitted; refresh history to check the result".into(),
        ));
    }
    // Claim before network IO. Failed/uncertain requests are never automatically resubmitted.
    let changed = s.db.lock().unwrap().execute(
        "UPDATE actions SET state='submitted' WHERE id=? AND state='pending' AND expires>?",
        params![id, now()],
    )?;
    if changed != 1 {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Action already submitted".into(),
        ));
    }
    let (community, config) = if matches!(p.operation, Operation::OwnLocation) {
        (location::own(&s, &p).await?, None)
    } else {
        upstream(&s, &p, &input.auth_chain).await?
    };
    match &p.operation {
        Operation::UpdateCommunity {
            community_id,
            thumbnail: Some(_),
            ..
        } => {
            images::invalidate_community(community_id);
            s.memo.forget("communities:");
        }
        Operation::CreateCommunity { .. } | Operation::UpdateCommunity { .. } => {
            s.memo.forget("communities:")
        }
        Operation::CreateEvent { .. } => s.memo.forget("events:"),
        Operation::EventRsvp { event_id, .. } => s.memo.forget(&format!("event:{event_id}")),
        _ => {}
    }
    if p.expires_at <= now() {
        return Err(ApiError(
            StatusCode::GONE,
            "Action expired while awaiting Foundation".into(),
        ));
    }
    match &p.operation {
        Operation::CommunityReports { community_id } => Ok(Json(
            json!({"reports":conversation::reports(&s,community_id)?}),
        )),
        Operation::MessageAction {
            community_id,
            message_id,
            action,
        } => {
            conversation::act(
                &s,
                community_id,
                message_id,
                &p.wallet,
                action,
                community["data"]["role"].as_str().unwrap_or(""),
            )?;
            Ok(Json(json!({"ok":true})))
        }
        Operation::Reply {
            community_id,
            message_id,
            text,
        } => {
            conversation::reply(&s, community_id, message_id, &id, &p.wallet, text)?;
            Ok(Json(json!({"id":id})))
        }
        Operation::ManageCommunity { .. }
        | Operation::CommunityRequests { .. }
        | Operation::ResolveRequest { .. }
        | Operation::CreateCommunity { .. }
        | Operation::UpdateCommunity { .. }
        | Operation::CommunityMembers { .. }
        | Operation::ActiveCommunityVoiceChats
        | Operation::CommunityPlaces { .. }
        | Operation::AddCommunityPlaces { .. }
        | Operation::RemoveCommunityPlace { .. }
        | Operation::CreateEvent { .. }
        | Operation::MyEvents
        | Operation::EventAttendees { .. }
        | Operation::EventRsvp { .. }
        | Operation::MyCommunities
        | Operation::MyJoinRequests { .. }
        | Operation::MyCommunityInvitations { .. }
        | Operation::CancelCommunityRequest { .. }
        | Operation::PrivateChatToken
        | Operation::OwnLocation
        | Operation::CommunityDetails { .. }
        | Operation::JoinCommunity { .. }
        | Operation::RequestCommunityJoin { .. }
        | Operation::CommunityPosts { .. }
        | Operation::PublishPost { .. } => Ok(Json(community)),
        Operation::ConfigureChannel {
            community_id,
            channel,
            config,
        } => {
            conversation::configure(&s, community_id, channel, config)?;
            Ok(Json(json!({"ok":true})))
        }
        Operation::CreateChannel {
            community_id,
            name,
            private,
        } => {
            conversation::create_channel(&s, community_id, name, *private)?;
            Ok(Json(json!({"ok":true})))
        }
        Operation::OpenCommunity {
            community_id,
            channel,
        } => {
            // A short-lived read capability carries the verified upstream proof. Every
            // poll rechecks Foundation membership; removal is not hidden by a role cache.
            let ticket = Uuid::new_v4().to_string();
            let record = json!({"prepared":p,"chain":input.auth_chain}).to_string();
            // Every poll replays this signature upstream, so the capability ends while
            // Foundation still honours it.
            let expires = p.timestamp.parse::<i64>().map_or(p.expires_at, |signed| {
                p.expires_at.min(signed + READ_WINDOW_MS)
            });
            s.db.lock().unwrap().execute("INSERT INTO actions(id,wallet,operation,prepared,expires,state) VALUES (?,?,?,?,?,'read')",params![ticket,p.wallet,"read",record,expires])?;
            Ok(Json(
                json!({"community":community["data"],"messages":channel_messages(&s,community_id,0,channel)?,"channels":conversation::channels(&s,community_id,community["data"]["role"].as_str().unwrap_or(""),&p.wallet)?,"channelConfig":conversation::configuration(&s,community_id,channel)?,"channel":channel,"readToken":ticket,"expiresAt":expires}),
            ))
        }
        Operation::SendMessage {
            community_id,
            channel,
            text,
            scene,
        } => {
            let created = now();
            let db = s.db.lock().unwrap();
            conversation::check_slow_mode(&db, config, community_id, channel, &p.wallet)?;
            db.execute(
                if channel == "general" { "INSERT INTO messages(id,community,wallet,text,scene,created,channel) VALUES (?,?,?,?,?,?,?)" } else { "INSERT INTO channel_messages(id,community,wallet,text,scene,created,channel) VALUES (?,?,?,?,?,?,?)" },
                params![
                    id,
                    community_id,
                    p.wallet,
                    text,
                    scene.as_ref().map(|v| serde_json::to_string(v).unwrap()),
                    created,
                    channel
                ],
            )?;
            Ok(Json(json!({"id":id,"createdAt":created})))
        }
    }
}
#[cfg(test)]
fn messages(s: &Store, community: &str, after: i64) -> ApiResult<Vec<Value>> {
    channel_messages(s, community, after, "general")
}
fn channel_messages(
    s: &Store,
    community: &str,
    after: i64,
    channel: &str,
) -> ApiResult<Vec<Value>> {
    Ok(message_page(
        s,
        community,
        channel,
        &HistoryQuery {
            after,
            ..Default::default()
        },
    )?
    .0)
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct HistoryQuery {
    after: i64,
    before: Option<i64>,
    search: Option<String>,
    pinned: bool,
    message: Option<String>,
    thread: Option<String>,
    reply_before: Option<i64>,
}
fn message_page(
    s: &Store,
    community: &str,
    channel: &str,
    q: &HistoryQuery,
) -> ApiResult<(Vec<Value>, bool)> {
    let db = s.db.lock().unwrap();
    let table = if channel == "general" {
        "messages"
    } else {
        "channel_messages"
    };
    let mut stmt = db.prepare(&format!("SELECT m.seq,m.id,m.wallet,m.text,m.scene,m.created FROM {table} m WHERE m.community=?1 AND m.channel=?2 AND m.seq>?3 AND (?4 IS NULL OR m.seq<?4) AND (?5 IS NULL OR instr(lower(m.text),lower(?5))>0 OR instr(lower(m.wallet),lower(?5))>0 OR EXISTS(SELECT 1 FROM replies r WHERE r.parent=m.id AND instr(lower(r.text),lower(?5))>0)) AND (?6=0 OR EXISTS(SELECT 1 FROM pins p WHERE p.message=m.id)) AND (?7 IS NULL OR m.id=?7) ORDER BY m.seq DESC LIMIT 101"))?;
    let mut result = stmt.query_map(params![community,channel,q.after.max(0),q.before,q.search,q.pinned,q.message],|r| {
        let scene:Option<String>=r.get(4)?;
        Ok(json!({"seq":r.get::<_,i64>(0)?,"id":r.get::<_,String>(1)?,"wallet":r.get::<_,String>(2)?,"text":r.get::<_,String>(3)?,"scene":scene.and_then(|s|serde_json::from_str::<Value>(&s).ok()),"createdAt":r.get::<_,i64>(5)?}))
    })?.collect::<Result<Vec<_>,_>>()?;
    let more = result.len() > 100;
    result.truncate(100);
    result.reverse();
    drop(stmt);
    conversation::enrich(&db, &mut result)?;
    Ok((result, more))
}
async fn history(
    State(s): State<Arc<Store>>,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    if q.search.as_ref().is_some_and(|s| s.len() > 400)
        || q.before.is_some_and(|v| v < 1)
        || q.reply_before.is_some_and(|v| v < 1)
    {
        return Err(bad("Invalid history query"));
    }
    for id in [&q.message, &q.thread].into_iter().flatten() {
        Uuid::parse_str(id).map_err(|_| bad("Invalid message ID"))?;
    }
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| denied("Open this community to read messages"))?;
    let row: Option<(String, i64)> =
        s.db.lock()
            .unwrap()
            .query_row(
                "SELECT prepared,expires FROM actions WHERE id=? AND state='read'",
                [token],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
    let (record, expires) = row.ok_or_else(|| denied("Open this community again"))?;
    if expires <= now() {
        return Err(ApiError(
            StatusCode::GONE,
            "Refresh this community to continue".into(),
        ));
    }
    let record: Value = serde_json::from_str(&record).map_err(|_| bad("Invalid read proof"))?;
    let p: Prepared = serde_json::from_value(record["prepared"].clone())
        .map_err(|_| bad("Invalid read proof"))?;
    let chain: AuthChain =
        serde_json::from_value(record["chain"].clone()).map_err(|_| bad("Invalid read proof"))?;
    if p.operation.community() != Some(id.as_str()) {
        return Err(denied("Read proof belongs to another community"));
    }
    verify(&p, &chain).await?;
    upstream(&s, &p, &chain).await?;
    if expires <= now() {
        return Err(ApiError(
            StatusCode::GONE,
            "Refresh this community to continue".into(),
        ));
    }
    if let Some(parent) = &q.thread {
        let (messages, _) = message_page(
            &s,
            &id,
            p.operation.channel(),
            &HistoryQuery {
                message: Some(parent.clone()),
                ..Default::default()
            },
        )?;
        if messages.is_empty() {
            return Err(denied("Thread does not belong to this channel"));
        }
        let (replies, has_more) =
            conversation::reply_page(&s.db.lock().unwrap(), parent, q.reply_before)?;
        return Ok(Json(
            json!({"replies":replies,"hasMore":has_more,"parent":messages[0]}),
        ));
    }
    let (messages, has_more) = message_page(&s, &id, p.operation.channel(), &q)?;
    Ok(Json(json!({"messages":messages,"hasMore":has_more})))
}

#[cfg(test)]
mod tests;

async fn profile(
    State(s): State<Arc<Store>>,
    Path(wallet): Path<String>,
) -> ApiResult<Json<Value>> {
    if wallet.len() != 42
        || !wallet.starts_with("0x")
        || !wallet[2..].bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(bad("Invalid wallet"));
    }
    let key = format!("profile:{}", wallet.to_lowercase());
    let value = memoized(&s, key, 300_000, || async {
        let response = s
            .client
            .get(format!(
                "https://peer.decentraland.org/lambdas/profile/{wallet}"
            ))
            .send()
            .await
            .map_err(|_| unavailable())?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        bounded_json(response).await
    })
    .await?;
    Ok(Json(value))
}
#[derive(Deserialize)]
struct PlaceQuery {
    position: Option<String>,
    #[serde(default)]
    search: String,
}
async fn places(
    State(s): State<Arc<Store>>,
    Query(q): Query<PlaceQuery>,
) -> ApiResult<Json<Value>> {
    if q.search.len() > 200 {
        return Err(bad("Search is too long"));
    }
    let mut request = s.client.get("https://places.decentraland.org/api/places");
    let key;
    if let Some(position) = q.position {
        let values: Vec<_> = position.split(',').collect();
        if values.len() != 2
            || values.iter().any(|v| {
                v.parse::<i16>()
                    .map_or(true, |n| !(-150..=150).contains(&n))
            })
        {
            return Err(bad("Invalid position"));
        }
        key = format!("places:position:{position}");
        request = request.query(&[("positions", position.as_str()), ("limit", "1")]);
    } else {
        key = format!("places:search:{}", q.search);
        request = request.query(&[("search", q.search.as_str()), ("limit", "8")]);
    }
    let value = memoized(&s, key, 30_000, || async {
        let response = request.send().await.map_err(|_| unavailable())?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        bounded_json(response).await
    })
    .await?;
    Ok(Json(value))
}
