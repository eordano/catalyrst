#![allow(clippy::result_large_err)]

pub mod auth_chain;
pub mod config;
pub mod content_store;
pub mod events;
pub mod fed;
pub mod handlers;
pub mod http;
pub mod ports;
pub mod validate;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{delete, get, patch, post};
use axum::Router;
use catalyrst_fed::sig::domains;
use catalyrst_fed::sig::Eip712Domain;
use catalyrst_fed::RateLimiter;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::config::Config;
use crate::content_store::ContentStore;
use crate::fed::replay::Replay;
use crate::ports::bans::BansComponent;
use crate::ports::communities::CommunitiesComponent;
use crate::ports::invites::InvitesComponent;
use crate::ports::members::MembersComponent;
use crate::ports::moderation::ModerationComponent;
use crate::ports::places::PlacesComponent;
use crate::ports::places_api::PlacesApiClient;
use crate::ports::posts::PostsComponent;
use crate::ports::profiles::ProfilesComponent;
use crate::ports::requests::RequestsComponent;
use crate::ports::voice::VoiceComponent;

pub struct AppStateInner {
    pub admin_token: Option<String>,
    pub bans: BansComponent,
    pub communities: CommunitiesComponent,
    pub invites: InvitesComponent,
    pub members: MembersComponent,
    pub moderation: ModerationComponent,
    pub places: PlacesComponent,
    pub places_api: PlacesApiClient,
    pub posts: PostsComponent,
    pub profiles: Arc<ProfilesComponent>,
    pub requests: RequestsComponent,
    pub voice: VoiceComponent,
    pub pool: PgPool,
    pub mutes_pool: Option<PgPool>,
    pub replay: Arc<Replay>,
    pub limiter: Arc<RateLimiter>,
    pub gossip: Arc<dyn catalyrst_fed::GossipPublisher>,
    pub domain: Eip712Domain,
    pub content_store: Arc<ContentStore>,
    pub cdn_url: String,
    pub global_moderators: Vec<String>,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Some(Duration::from_secs(60)))
        .connect(&cfg.database_url)
        .await
        .context("failed to connect to communities database")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("communities migration failed")?;

    let replay = Replay::new(pool.clone())
        .await
        .context("failed to load replay state")?;
    let limiter = Arc::new(RateLimiter::new(60, Duration::from_secs(60)));

    let gossip = catalyrst_fed::build_publisher(&catalyrst_fed::GossipConfig::from_env()).await;
    tracing::info!(
        gossip_live = gossip.is_live(),
        "communities gossip publisher ready"
    );

    let content_store = Arc::new(ContentStore::new(cfg.communities_content_dir.clone()));
    content_store.init().await.with_context(|| {
        format!(
            "failed to init content dir at {:?}",
            cfg.communities_content_dir
        )
    })?;
    tracing::info!(
        path = %cfg.communities_content_dir.display(),
        "communities content store ready"
    );

    let content_pool = match &cfg.content_database_url {
        Some(url) => match PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Some(Duration::from_secs(60)))
            .connect(url)
            .await
        {
            Ok(p) => {
                tracing::info!("connected to content DB for profile enrichment");
                Some(p)
            }
            Err(e) => {
                tracing::warn!(error = %e, "content DB unavailable; profile enrichment disabled");
                None
            }
        },
        None => {
            tracing::info!("CONTENT_PG_CONNECTION_STRING unset; profile enrichment disabled");
            None
        }
    };
    let mutes_pool = match &cfg.mutes_database_url {
        Some(url) => match PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Some(Duration::from_secs(60)))
            .connect(url)
            .await
        {
            Ok(p) => {
                tracing::info!("connected to social DB for /v1/mutes");
                Some(p)
            }
            Err(e) => {
                tracing::warn!(error = %e, "social DB unavailable; /v1/mutes disabled");
                None
            }
        },
        None => {
            tracing::info!("MUTES_PG_CONNECTION_STRING unset; /v1/mutes disabled");
            None
        }
    };

    let profiles = Arc::new(ProfilesComponent::new(
        content_pool,
        cfg.content_server_address.clone(),
    ));

    let state = Arc::new(AppStateInner {
        admin_token: cfg.admin_token.clone(),
        bans: BansComponent::new(pool.clone()),
        communities: CommunitiesComponent::new(pool.clone()),
        invites: InvitesComponent::new(pool.clone()),
        members: MembersComponent::new(pool.clone()),
        moderation: ModerationComponent::new(pool.clone()),
        places: PlacesComponent::new(pool.clone()),
        places_api: PlacesApiClient::new(cfg.places_api_url.clone()),
        posts: PostsComponent::new(pool.clone()),
        profiles,
        requests: RequestsComponent::new(pool.clone()),
        voice: VoiceComponent::new(pool.clone()),
        pool: pool.clone(),
        mutes_pool,
        replay,
        limiter,
        gossip,
        domain: domains::communities(),
        content_store,
        cdn_url: cfg.cdn_url.trim_end_matches('/').to_string(),
        global_moderators: cfg.global_moderators.clone(),
    });

    crate::fed::consumer::spawn(state.clone()).await;

    Ok(state)
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/communities",
            get(handlers::communities::get_communities).post(handlers::writes::create_community),
        )
        .route(
            "/v1/mutes",
            get(handlers::mutes::get_mutes)
                .post(handlers::mutes::add_mute)
                .delete(handlers::mutes::remove_mute),
        )
        .route("/v1/friends", get(handlers::friends::list_friends))
        .route(
            "/v1/friends/{peer}/messages",
            get(handlers::friends::get_messages).post(handlers::friends::send_message),
        )
        .route(
            "/social/communities/{id}/raw-thumbnail.png",
            get(handlers::communities::get_raw_thumbnail),
        )
        .route(
            "/v1/communities/{id}",
            get(handlers::communities::get_community)
                .put(handlers::writes::update_community)
                .patch(handlers::writes::update_community_partially)
                .delete(handlers::writes::delete_community),
        )
        .route(
            "/v1/communities/{id}/members",
            get(handlers::members::get_members).post(handlers::writes::add_member),
        )
        .route(
            "/v1/communities/{id}/members/{address}",
            delete(handlers::writes::remove_member).patch(handlers::writes::update_member_role),
        )
        .route("/v1/communities/{id}/bans", get(handlers::bans::get_bans))
        .route(
            "/v1/communities/{id}/members/{address}/bans",
            post(handlers::writes::ban_member).delete(handlers::writes::unban_member),
        )
        .route(
            "/v1/communities/{id}/places",
            get(handlers::places::get_places).post(handlers::writes::add_places),
        )
        .route(
            "/v1/communities/{id}/places/{placeId}",
            delete(handlers::writes::remove_place),
        )
        .route(
            "/v1/communities/{id}/posts",
            get(handlers::posts::get_posts).post(handlers::writes::create_post),
        )
        .route(
            "/v1/communities/{id}/posts/{postId}",
            delete(handlers::writes::delete_post),
        )
        .route(
            "/v1/communities/{id}/posts/{postId}/like",
            post(handlers::writes::like_post).delete(handlers::writes::unlike_post),
        )
        .route(
            "/v1/communities/{id}/requests",
            get(handlers::requests::get_community_requests).post(handlers::writes::create_request),
        )
        .route(
            "/v1/communities/{id}/requests/{requestId}",
            patch(handlers::writes::update_request_status),
        )
        .route(
            "/v1/communities/{address}/managed",
            get(handlers::members::get_managed_communities),
        )
        .route(
            "/v1/members/{address}/communities",
            get(handlers::members::get_member_communities)
                .post(handlers::writes::member_communities_by_ids),
        )
        .route(
            "/v1/members/{address}/requests",
            get(handlers::requests::get_member_requests),
        )
        .route(
            "/v1/members/{address}/invites",
            get(handlers::invites::get_invites),
        )
        .route(
            "/v1/community-voice-chats/active",
            get(handlers::voice::get_active_voice_chats),
        )
        .route(
            "/v1/moderation/communities",
            get(handlers::moderation::get_moderation_communities),
        )
        .route(
            "/v2/communities",
            get(handlers::communities::get_communities_v2),
        )
        .route(
            "/v2/communities/{id}",
            get(handlers::communities::get_community_v2),
        )
        .route(
            "/v2/communities/{id}/members",
            get(handlers::members::get_members_v2),
        )
        .route(
            "/v2/communities/{id}/bans",
            get(handlers::bans::get_bans_v2),
        )
        .route(
            "/v2/communities/{id}/requests",
            get(handlers::requests::get_community_requests_v2),
        )
        .route(
            "/v2/communities/{id}/posts",
            get(handlers::posts::get_posts_v2),
        )
        .route(
            "/v2/members/{address}/requests",
            get(handlers::requests::get_member_requests_v2),
        )
        .route(
            "/v1/admin/communities",
            get(handlers::admin::list_communities),
        )
        .route(
            "/v1/admin/communities/{id}/suspend",
            post(handlers::admin::suspend_community),
        )
        .route(
            "/v1/admin/communities/{id}/unsuspend",
            post(handlers::admin::unsuspend_community),
        )
        .route(
            "/federation/communities/snapshot",
            get(handlers::federation::snapshot),
        )
        .route(
            "/federation/communities/changes",
            get(handlers::federation::changes),
        )
        .route(
            "/federation/communities/content",
            post(handlers::content::put_content),
        )
        .route(
            "/federation/communities/content/gc",
            post(handlers::content::gc_content),
        )
        .route(
            "/federation/communities/content/{hash}",
            get(handlers::content::get_content),
        )
        .layer(axum::extract::DefaultBodyLimit::max(512 * 1024))
}
