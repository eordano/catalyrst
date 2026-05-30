pub mod auth_chain;
pub mod config;
pub mod extract;
pub mod handlers;
pub mod http;
pub mod livekit;
pub mod mls;
pub mod moderator;
pub mod ports;
pub mod scene_perms;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{delete, get, patch, post};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

use crate::config::Config;

/// Parse a connection URL and pin server-side query/transaction timeouts so a
/// pathological MLS/community query can't tie up a pooled connection
/// indefinitely. Mirrors the `statement_timeout`/`idle_in_transaction` knobs
/// the catalyrst-db and camera-reel pools already set.
fn connect_opts(url: &str) -> Result<PgConnectOptions> {
    Ok(url
        .parse::<PgConnectOptions>()
        .context("invalid postgres connection string")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]))
}
use crate::ports::names::NamesComponent;
use crate::ports::scene_admin::SceneAdminComponent;
use crate::ports::scene_bans::SceneBansComponent;
use crate::ports::user_bans::UserBansComponent;

pub struct AppStateInner {
    pub pool: PgPool,
    pub scene_admin: SceneAdminComponent,
    pub scene_bans: SceneBansComponent,
    pub user_bans: UserBansComponent,
    pub names: NamesComponent,
    pub http: reqwest::Client,
    pub catalyst_url: String,
    pub livekit_host: String,
    pub livekit_ws_url: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub livekit_webhook_key: Option<String>,
    pub livekit_configured: bool,
    pub private_messages_room_id: String,
    pub authoritative_server_address: Option<String>,
    pub moderator_token: Option<String>,
    pub moderator_addresses: Vec<String>,
    /// Bearer token gating the voice / private-message social-service routes
    /// (upstream `COMMS_GATEKEEPER_AUTH_TOKEN`). `None` => gate disabled (loopback
    /// dev). See `voice_auth_layer`.
    pub gatekeeper_auth_token: Option<String>,
    /// places_events archive pool — resolves place_id -> parcels/world for the
    /// scene-bans/scene-admin owner-or-admin authz (see `scene_perms`).
    pub places_pool: Option<PgPool>,
    /// marketplace squid pool — resolves parcel/estate/ENS ownership for authz.
    pub dapps_pool: Option<PgPool>,
    /// squid schema (e.g. `squid_marketplace`).
    pub dapps_schema: String,
}

impl AppStateInner {
    pub fn room_service(&self) -> crate::livekit::RoomServiceClient<'_> {
        crate::livekit::RoomServiceClient::new(
            &self.http,
            &self.livekit_host,
            &self.livekit_api_key,
            &self.livekit_api_secret,
        )
    }
}

pub type AppState = Arc<AppStateInner>;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Some(Duration::from_secs(60)))
        .connect_with(connect_opts(&cfg.database_url)?)
        .await
        .context("failed to connect to comms database")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("comms migration failed")?;

    let dapps_pool = match cfg.dapps_database_url.as_deref() {
        Some(url) => match PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Some(Duration::from_secs(60)))
            .connect_with(connect_opts(url)?)
            .await
        {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!(error = %e, "failed to connect to squid marketplace pool; name enrichment disabled");
                None
            }
        },
        None => {
            tracing::info!("DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING unset; name enrichment disabled (names resolve to \"\")");
            None
        }
    };

    // places_events archive pool — for scene-bans/admin owner-or-admin authz.
    let places_pool = match cfg.places_database_url.as_deref() {
        Some(url) => match PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Some(Duration::from_secs(60)))
            .connect_with(connect_opts(url)?)
            .await
        {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!(error = %e, "failed to connect to places_events pool; scene-bans/admin owner resolution degraded (admins still work, non-admins denied)");
                None
            }
        },
        None => {
            tracing::warn!("PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING unset; scene-bans/admin owner resolution disabled (only explicit scene admins can moderate)");
            None
        }
    };

    if cfg.authoritative_server_address.is_none() {
        tracing::warn!(
            "AUTHORITATIVE_SERVER_ADDRESS unset; POST /get-server-scene-adapter rejects every caller (no authoritative server identity configured)"
        );
    }

    if cfg.moderator_token.is_none() && cfg.moderator_addresses.is_empty() {
        tracing::warn!(
            "neither MODERATOR_TOKEN nor PLATFORM_USER_MODERATORS set; user-moderation write routes (POST/DELETE /users/:address/bans, /warnings, GET /bans) will reject every caller as unauthorized"
        );
    }

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to build comms http client")?;

    Ok(Arc::new(AppStateInner {
        scene_admin: SceneAdminComponent::new(pool.clone()),
        scene_bans: SceneBansComponent::new(pool.clone()),
        user_bans: UserBansComponent::new(pool.clone()),
        names: NamesComponent::new(dapps_pool.clone(), cfg.dapps_schema.clone()),
        places_pool,
        dapps_pool,
        dapps_schema: cfg.dapps_schema.clone(),
        http,
        catalyst_url: cfg.catalyst_url.trim_end_matches('/').to_string(),
        pool: pool.clone(),
        livekit_host: cfg.livekit_host.clone(),
        livekit_ws_url: std::env::var("LIVEKIT_WS_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("wss://{}", cfg.livekit_host)),
        livekit_api_key: cfg.livekit_api_key.clone(),
        livekit_api_secret: cfg.livekit_api_secret.clone(),
        livekit_webhook_key: cfg.livekit_webhook_key.clone(),
        livekit_configured: cfg.livekit_configured,
        private_messages_room_id: cfg.private_messages_room_id.clone(),
        authoritative_server_address: cfg.authoritative_server_address.clone(),
        moderator_token: cfg.moderator_token.clone(),
        moderator_addresses: cfg.moderator_addresses.clone(),
        gatekeeper_auth_token: cfg.gatekeeper_auth_token.clone(),
    }))
}

/// Bearer-token middleware for the voice / private-message social-service routes.
///
/// Upstream comms-gatekeeper gates these routes behind `tokenAuthMiddleware`
/// (`COMMS_GATEKEEPER_AUTH_TOKEN`). Here the gate is OPT-IN: when
/// `gatekeeper_auth_token` is `None` the request is allowed through (the warning
/// is logged once at startup in `build_state`'s caller), so the loopback dev
/// stack keeps working. When the token IS set, every request must carry a
/// matching `Authorization: Bearer <token>` (constant-time compared) or it is
/// rejected with 401. `/private-messages/token` is intentionally NOT behind this
/// layer — upstream gates it with signed-fetch (`authExplorer`), which its
/// handler already enforces via `try_extract_signer`.
/// Paths that upstream comms-gatekeeper gates behind `tokenAuthMiddleware`
/// (the `socialServiceInteractionsToken` bearer). Covers /private-voice-chat*,
/// /community-voice-chat*, .../voice-chat-status, and the
/// /users/{address}/private-messages-privacy write. `/private-messages/token`
/// is intentionally excluded — upstream gates it with signed-fetch (authExplorer),
/// enforced by the handler's `try_extract_signer`.
pub(crate) fn is_bearer_gated_path(path: &str) -> bool {
    path.contains("voice-chat") || path.contains("private-messages-privacy")
}

pub async fn voice_auth_layer(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    // Only the voice-chat routes are gated (the paths upstream comms-gatekeeper
    // protects with COMMS_GATEKEEPER_AUTH_TOKEN). Everything else — scene-adapter,
    // scene-bans/admin, MLS, /private-messages/token — keeps its existing
    // signed-fetch / per-handler auth. Matching on the "voice-chat" path segment
    // covers /private-voice-chat*, /community-voice-chat*, and the
    // .../voice-chat-status reads. PATCH /users/{address}/private-messages-privacy
    // is also gated by upstream's tokenAuthMiddleware but its path has no
    // "voice-chat" segment, so match it explicitly (else it would be open when
    // the bearer token is enabled, unlike upstream).
    let is_voice_route = is_bearer_gated_path(req.uri().path());
    if is_voice_route {
        if let Some(expected) = state.gatekeeper_auth_token.as_deref() {
            let ok = crate::moderator::bearer_token(req.headers())
                .map(|t| crate::moderator::timing_safe_eq(&t, expected))
                .unwrap_or(false);
            if !ok {
                return crate::http::unauthorized("Authentication required").into_response();
            }
        }
    }
    next.run(req).await
}

pub fn api_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/get-scene-adapter", post(handlers::scene_adapter::get_scene_adapter))
        .route("/get-server-scene-adapter", post(handlers::scene_adapter::get_server_scene_adapter))
        .route("/scene-participants", get(handlers::scene_participants::list_participants))
        .route(
            "/scene-admin",
            get(handlers::scene_admin::list_admins)
                .post(handlers::scene_admin::add_admin)
                .delete(handlers::scene_admin::remove_admin),
        )
        .route(
            "/scene-bans",
            get(handlers::scene_bans::list_bans)
                .post(handlers::scene_bans::ban_user)
                .delete(handlers::scene_bans::unban_user),
        )
        .route("/scene-bans/addresses", get(handlers::scene_bans::list_ban_addresses))
        .route(
            "/users/{address}/bans",
            get(handlers::user_bans::get_user_ban_status)
                .post(handlers::user_bans::post_user_ban)
                .delete(handlers::user_bans::delete_user_ban),
        )
        .route(
            "/users/{address}/warnings",
            get(handlers::user_bans::get_user_warnings).post(handlers::user_bans::post_user_warning),
        )
        .route("/bans", get(handlers::user_bans::list_all_bans))
        .route("/livekit-webhook", post(handlers::webhook::livekit_webhook))
        .route("/private-messages/token", get(handlers::voice::private_messages_token))
        .route(
            "/users/{address}/private-messages-privacy",
            patch(handlers::voice::patch_private_messages_privacy),
        )
        .route(
            "/private-voice-chat",
            post(handlers::voice::create_private_voice_chat),
        )
        .route(
            "/users/{address}/voice-chat-status",
            get(handlers::voice::get_voice_chat_status),
        )
        .route(
            "/private-voice-chat/{id}",
            delete(handlers::voice::end_private_voice_chat),
        )
        .route(
            "/users/{address}/community-voice-chat-status",
            get(handlers::voice::check_user_community_status),
        )
        .route("/community-voice-chat", post(handlers::voice::community_voice_chat_create_or_join))
        .route(
            "/community-voice-chat/{id}/status",
            get(handlers::voice::community_voice_chat_status),
        )
        .route("/community-voice-chat/status", post(handlers::voice::community_voice_chat_bulk_status))
        .route("/community-voice-chat/active", get(handlers::voice::community_voice_chat_active))
        .route(
            "/community-voice-chat/{id}/users/{address}/speak-request",
            post(handlers::voice::community_request_to_speak)
                .delete(handlers::voice::community_reject_speak_request),
        )
        .route(
            "/community-voice-chat/{id}/users/{address}/speaker",
            post(handlers::voice::community_promote_speaker)
                .delete(handlers::voice::community_demote_speaker),
        )
        .route(
            "/community-voice-chat/{id}/users/{address}/mute",
            patch(handlers::voice::community_mute_speaker),
        )
        .route(
            "/community-voice-chat/{id}/users/{address}",
            delete(handlers::voice::community_kick_player),
        )
        .route(
            "/community-voice-chat/{id}",
            delete(handlers::voice::community_voice_chat_end),
        )
        // --- MLS messaging (RFC 9420) delivery service — docs/federation/messaging.md
        .route("/mls/key-packages", post(handlers::messaging::publish_key_packages))
        .route(
            "/mls/key-packages/{owner}",
            get(handlers::messaging::claim_key_package),
        )
        .route(
            "/mls/key-packages/{owner}/count",
            get(handlers::messaging::key_package_count),
        )
        .route("/mls/groups", post(handlers::messaging::create_group))
        .route(
            "/mls/groups/{group_id}/commits",
            get(handlers::messaging::fetch_commits).post(handlers::messaging::submit_commit),
        )
        .route(
            "/mls/groups/{group_id}/messages",
            get(handlers::messaging::fetch_history).post(handlers::messaging::send_message),
        )
        .route("/mls/blobs/{hash}", get(handlers::messaging::fetch_blob))
        .route("/cast/generate-stream-link", get(handlers::deferred::cast_any))
        .route("/cast/stream-info/{key}", get(handlers::deferred::cast_any))
        .route("/cast/streamer-token", post(handlers::deferred::cast_any))
        .route("/cast/watcher-token", post(handlers::deferred::cast_any))
        .route("/cast/presentation-bot-token", post(handlers::deferred::cast_any))
        .route("/cast/presenters", get(handlers::deferred::cast_any))
        .route(
            "/cast/presenters/{id}",
            axum::routing::put(handlers::deferred::cast_any).delete(handlers::deferred::cast_any),
        )
        .route(
            "/scene-stream-access",
            axum::routing::put(handlers::deferred::scene_stream_access_put_delete)
                .delete(handlers::deferred::scene_stream_access_put_delete),
        )
        .layer(axum::extract::DefaultBodyLimit::max(512 * 1024))
        // Bearer-token gate on the voice-chat routes (opt-in via
        // COMMS_GATEKEEPER_AUTH_TOKEN; no-op when unset — loopback dev unaffected).
        .layer(axum::middleware::from_fn_with_state(state, voice_auth_layer))
}

#[cfg(test)]
mod bearer_gate_tests {
    use super::is_bearer_gated_path;
    #[test]
    fn bearer_gated_paths_match_upstream_tokenauthmiddleware() {
        // gated (upstream tokenAuthMiddleware)
        assert!(is_bearer_gated_path("/community-voice-chat"));
        assert!(is_bearer_gated_path("/community-voice-chat/status"));
        assert!(is_bearer_gated_path("/community-voice-chat/active"));
        assert!(is_bearer_gated_path("/community-voice-chat/abc/users/0x1/mute"));
        assert!(is_bearer_gated_path("/private-voice-chat"));
        assert!(is_bearer_gated_path("/private-voice-chat/id123"));
        assert!(is_bearer_gated_path("/users/0xabc/voice-chat-status"));
        assert!(is_bearer_gated_path("/users/0xabc/community-voice-chat-status"));
        assert!(is_bearer_gated_path("/users/0xabc/private-messages-privacy"));
        // NOT gated (signed-fetch / per-handler / public)
        assert!(!is_bearer_gated_path("/private-messages/token"));
        assert!(!is_bearer_gated_path("/get-scene-adapter"));
        assert!(!is_bearer_gated_path("/scene-participants"));
        assert!(!is_bearer_gated_path("/scene-bans"));
        assert!(!is_bearer_gated_path("/mls/groups"));
        assert!(!is_bearer_gated_path("/ping"));
    }
}
