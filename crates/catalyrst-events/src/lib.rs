pub mod admin;
pub mod auth_chain;
pub mod config;
pub mod handlers;
pub mod http;
pub mod ports;
pub mod schemas;
pub mod fed;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::config::Config;
use crate::ports::attendees::AttendeesComponent;
use crate::ports::categories::CategoriesComponent;
use crate::ports::events::EventsComponent;
use crate::ports::schedules::SchedulesComponent;

pub struct AppStateInner {
    pub events: EventsComponent,
    pub attendees: AttendeesComponent,
    pub categories: CategoriesComponent,
    pub schedules: SchedulesComponent,
    /// Bearer token gating admin moderation routes; `None` ⇒ fail closed (403).
    pub admin_token: Option<String>,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let opts = PgConnectOptions::from_str(&cfg.places_events_database_url)
        .context("invalid PLACES_EVENTS_PG_CONNECTION_STRING")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect places_events pool")?;

    Ok(Arc::new(AppStateInner {
        events: EventsComponent::new(pool.clone()),
        attendees: AttendeesComponent::new(pool.clone()),
        categories: CategoriesComponent::new(pool.clone()),
        schedules: SchedulesComponent::new(pool),
        admin_token: cfg.admin_token.clone(),
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/events",
            get(handlers::events::get_event_list).post(handlers::events::create_event),
        )
        .route(
            "/api/events/search",
            post(handlers::events::post_event_search),
        )
        .route(
            "/api/events/attending",
            get(handlers::events::get_attending_event_list),
        )
        .route(
            "/api/events/categories",
            get(handlers::categories::get_event_category_list),
        )
        .route(
            "/api/events/{event_id}",
            get(handlers::events::get_event).patch(handlers::events::patch_event),
        )
        .route(
            "/api/events/{event_id}/attendees",
            get(handlers::attendees::get_event_attendees)
                .post(handlers::attendees::create_event_attendee)
                .delete(handlers::attendees::delete_event_attendee),
        )
        .route(
            "/api/schedules",
            get(handlers::schedules::get_schedule_list).post(handlers::schedules::create_schedule),
        )
        .route(
            "/api/schedules/{schedule_id}",
            get(handlers::schedules::get_schedule_by_id).patch(handlers::schedules::patch_schedule),
        )
        .route("/api/poster", post(handlers::poster::upload_poster))
        .route(
            "/api/poster-vertical",
            post(handlers::poster::upload_poster_vertical),
        )
        .route(
            "/api/profiles/settings",
            get(handlers::profile_settings::list_profile_settings),
        )
        .route(
            "/api/profiles/me/settings",
            get(handlers::profile_settings::get_auth_profile_settings)
                .patch(handlers::profile_settings::update_my_profile_settings),
        )
        .route(
            "/api/profiles/{profile_id}/settings",
            get(handlers::profile_settings::get_profile_settings)
                .patch(handlers::profile_settings::update_profile_settings),
        )
        .route(
            "/api/profiles/subscriptions",
            get(handlers::profile_subscription::get_profile_subscription)
                .post(handlers::profile_subscription::create_profile_subscription)
                .delete(handlers::profile_subscription::delete_profile_subscription),
        )
        .route("/events/sitemap.xml", get(handlers::sitemap::sitemap_index))
        .route(
            "/events/sitemap.static.xml",
            get(handlers::sitemap::sitemap_static),
        )
        .route(
            "/events/sitemap.events.xml",
            get(handlers::sitemap::sitemap_events),
        )
        .route(
            "/events/sitemap.schedules.xml",
            get(handlers::sitemap::sitemap_schedules),
        )
        .route(
            "/federation/v1/events/feed",
            get(handlers::federation::get_feed),
        )
        .route(
            "/federation/v1/events/{event_id}/attendance",
            get(handlers::federation::get_attendance),
        )
}
