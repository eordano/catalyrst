#![allow(clippy::result_large_err)]

pub mod auth_chain;
pub mod community_membership_authority;
pub mod config;
pub mod content_store;
pub mod events;
pub mod fed;
pub mod handlers;
pub mod http;
pub mod openapi;
pub mod ports;
pub mod thumbnail_signature;
pub mod validate;

pub use openapi::api_router_with_spec;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::get;
use axum::Router;
use catalyrst_fed::sig::domains;
use catalyrst_fed::sig::Eip712Domain;
use catalyrst_fed::RateLimiter;
use sqlx::PgPool;

use crate::rest::config::Config;
use crate::rest::content_store::ContentStore;
use crate::rest::fed::replay::Replay;
use crate::rest::ports::bans::BansComponent;
use crate::rest::ports::communities::CommunitiesComponent;
use crate::rest::ports::invites::InvitesComponent;
use crate::rest::ports::members::MembersComponent;
use crate::rest::ports::moderation::ModerationComponent;
use crate::rest::ports::places::PlacesComponent;
use crate::rest::ports::places_api::PlacesApiClient;
use crate::rest::ports::posts::PostsComponent;
use crate::rest::ports::profiles::ProfilesComponent;
use crate::rest::ports::requests::RequestsComponent;
use crate::rest::ports::voice::VoiceComponent;

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
    let pool = catalyrst_db::connect_pool(
        &cfg.database_url,
        &catalyrst_db::PoolSettings::standard_service(),
    )
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

    let gossip = catalyrst_fed::build_publisher(&catalyrst_fed::GossipConfig::from_env()).await?;
    tracing::info!(
        gossip_live = gossip.is_live(),
        "communities gossip publisher ready"
    );

    let content_store = Arc::new(ContentStore::new(
        cfg.communities_content_dir.clone(),
        crate::rest::content_store::MAX_BODY_BYTES,
    ));
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
        Some(url) => {
            match catalyrst_db::connect_pool(url, &catalyrst_db::PoolSettings::side_pool()).await {
                Ok(p) => {
                    tracing::info!("connected to content DB for profile enrichment");
                    Some(p)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "content DB unavailable; profile enrichment disabled");
                    None
                }
            }
        }
        None => {
            tracing::info!("CONTENT_PG_CONNECTION_STRING unset; profile enrichment disabled");
            None
        }
    };
    let mutes_pool = match &cfg.mutes_database_url {
        Some(url) => {
            match catalyrst_db::connect_pool(url, &catalyrst_db::PoolSettings::side_pool()).await {
                Ok(p) => {
                    tracing::info!("connected to social DB for /v1/mutes");
                    Some(p)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "social DB unavailable; /v1/mutes disabled");
                    None
                }
            }
        }
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

    crate::rest::fed::consumer::spawn(state.clone()).await;

    Ok(state)
}

pub fn api_router() -> Router<AppState> {
    let (router, spec) = api_router_with_spec();
    router
        .route(
            "/openapi.json",
            get(move || {
                let spec = spec.clone();
                async move { axum::Json(spec) }
            }),
        )
        .layer(axum::extract::DefaultBodyLimit::max(512 * 1024))
}
