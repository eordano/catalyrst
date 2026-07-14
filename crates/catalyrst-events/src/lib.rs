pub mod admin;
pub mod auth_chain;
pub mod clients;
pub mod config;
pub mod content_store;
pub mod fed;
pub mod handlers;
pub mod http;
pub mod mirror;
pub mod ports;
pub mod sanitize;
pub mod schemas;

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::routing::get;
use axum::Router;
use catalyrst_fed::sig::Eip712Domain;
use sqlx::PgPool;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::clients::CommsGatekeeper;
use crate::config::Config;
use crate::content_store::ContentStore;
use crate::ports::attendees::AttendeesComponent;
use crate::ports::categories::CategoriesComponent;
use crate::ports::events::EventsComponent;
use crate::ports::schedules::SchedulesComponent;

pub struct AppStateInner {
    pub events: EventsComponent,
    pub attendees: AttendeesComponent,
    pub categories: CategoriesComponent,
    pub schedules: SchedulesComponent,

    pub admin_token: Option<String>,

    pub pool: PgPool,

    pub gossip: Arc<dyn catalyrst_fed::GossipPublisher>,

    pub domain: Eip712Domain,

    pub content_store: Arc<ContentStore>,

    pub comms: CommsGatekeeper,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = catalyrst_db::connect_pool(
        &cfg.places_events_database_url,
        &catalyrst_db::PoolSettings::default(),
    )
    .await
    .context("failed to connect places_events pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("events migration failed")?;

    let gossip = catalyrst_fed::build_publisher(&catalyrst_fed::GossipConfig::from_env()).await?;
    tracing::info!(
        gossip_live = gossip.is_live(),
        "events gossip publisher ready"
    );

    let content_store = Arc::new(ContentStore::new(
        cfg.content_dir.clone(),
        crate::content_store::MAX_POSTER_BYTES,
    ));
    content_store
        .init()
        .await
        .with_context(|| format!("failed to init content dir at {:?}", cfg.content_dir))?;

    let state = Arc::new(AppStateInner {
        events: EventsComponent::new(pool.clone(), cfg.asset_rewrite_domain.clone()),
        attendees: AttendeesComponent::new(pool.clone()),
        categories: CategoriesComponent::new(pool.clone()),
        schedules: SchedulesComponent::new(pool.clone()),
        admin_token: cfg.admin_token.clone(),
        pool,
        gossip,
        domain: catalyrst_fed::sig::domains::events(),
        content_store,
        comms: CommsGatekeeper::new(cfg.comms_gatekeeper_url.clone()),
    });

    crate::fed::consumer::spawn(state.clone()).await;

    if cfg.mirror_upstream {
        let interval = std::time::Duration::from_secs(cfg.mirror_interval_secs);
        crate::mirror::spawn(state.pool.clone(), cfg.upstream_url.clone(), interval);
        tracing::info!(
            upstream = %cfg.upstream_url,
            interval_secs = cfg.mirror_interval_secs,
            "event catalog: mirroring from upstream"
        );
    }

    Ok(state)
}

#[derive(OpenApi)]
#[openapi(info(title = "catalyrst-events"))]
struct ApiDoc;

pub fn api_router_with_spec() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(
            handlers::events::get_event_list,
            handlers::event_writes::create_event
        ))
        .routes(routes!(handlers::events::post_event_search))
        .routes(routes!(handlers::events::get_attending_event_list))
        .routes(routes!(handlers::events::get_moderation_list))
        .routes(routes!(handlers::categories::get_event_category_list))
        .routes(routes!(
            handlers::events::get_event,
            handlers::event_writes::patch_event,
            handlers::event_writes::delete_event
        ))
        .routes(routes!(
            handlers::attendees::get_event_attendees,
            handlers::attendees::create_event_attendee,
            handlers::attendees::delete_event_attendee
        ))
        .routes(routes!(
            handlers::schedules::get_schedule_list,
            handlers::schedules::create_schedule
        ))
        .routes(routes!(
            handlers::schedules::get_schedule_by_id,
            handlers::schedules::patch_schedule
        ))
        .routes(routes!(handlers::poster::upload_poster))
        .routes(routes!(handlers::poster::upload_poster_vertical))
        .routes(routes!(handlers::poster::get_poster))
        .routes(routes!(handlers::poster::get_poster_vertical))
        .routes(routes!(handlers::profile_settings::list_profile_settings))
        .routes(routes!(
            handlers::profile_settings::get_auth_profile_settings,
            handlers::profile_settings::update_my_profile_settings
        ))
        .routes(routes!(
            handlers::profile_settings::get_profile_settings,
            handlers::profile_settings::update_profile_settings
        ))
        .routes(routes!(
            handlers::profile_subscription::get_profile_subscription,
            handlers::profile_subscription::create_profile_subscription,
            handlers::profile_subscription::delete_profile_subscription
        ))
        .routes(routes!(handlers::sitemap::sitemap_index))
        .routes(routes!(handlers::sitemap::sitemap_static))
        .routes(routes!(handlers::sitemap::sitemap_events))
        .routes(routes!(handlers::sitemap::sitemap_schedules))
        .routes(routes!(handlers::federation::get_feed))
        .routes(routes!(handlers::federation::get_attendance))
        .split_for_parts()
}

pub fn api_router() -> Router<AppState> {
    let (router, spec) = api_router_with_spec();
    router.route(
        "/openapi.json",
        get(move || {
            let spec = spec.clone();
            async move { axum::Json(spec) }
        }),
    )
}

#[cfg(test)]
mod openapi_export {
    #[test]
    fn export_bindings_openapi() {
        let spec = super::api_router_with_spec().1;
        let rendered = serde_json::to_string_pretty(&spec).expect("spec serialises");
        catalyrst_contract_gate::assert_usable_spec(
            "events",
            &serde_json::from_str(&rendered).expect("spec round-trips through JSON"),
        );
        let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") else {
            return;
        };
        let out = std::path::Path::new(&dir).join("openapi");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("events.openapi.json"), rendered).unwrap();
    }
}
