pub mod admin;
pub mod auth_chain;
pub mod config;
pub mod first_wear;
pub mod handlers;
pub mod http;
pub mod ports;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, post, put};
use axum::Router;
use sqlx::postgres::PgPoolOptions;

use crate::config::Config;
use crate::ports::NotificationsComponent;

pub struct AppStateInner {
    pub notifications: NotificationsComponent,

    pub admin_token: Option<String>,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Some(Duration::from_secs(60)))
        .connect(&cfg.database_url)
        .await
        .context("failed to connect to notifications database")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("notifications migration failed")?;

    match (
        &cfg.content_database_url,
        &cfg.social_database_url,
        &cfg.squid_database_url,
    ) {
        (Some(content), Some(social), Some(squid)) => {
            let telemetry = match &cfg.telemetry_database_url {
                Some(url) => Some(
                    first_wear::connect_pool(url)
                        .await
                        .context("failed to connect first_wear telemetry pool")?,
                ),
                None => {
                    tracing::info!(
                        "TELEMETRY_PG_CONNECTION_STRING unset: ffw_rules uses default arms, no funnel events"
                    );
                    None
                }
            };
            let pools = first_wear::FirstWearPools {
                own: pool.clone(),
                content: first_wear::connect_pool(content)
                    .await
                    .context("failed to connect first_wear content pool")?,
                social: first_wear::connect_pool(social)
                    .await
                    .context("failed to connect first_wear social pool")?,
                squid: first_wear::connect_pool(squid)
                    .await
                    .context("failed to connect first_wear squid pool")?,
                telemetry,
            };
            first_wear::spawn_first_wear(pools, cfg.shop_item_base_url.clone());
            tracing::info!("friend_first_wear ingestion worker up");
        }
        _ => tracing::info!(
            "friend_first_wear ingestion off (set CONTENT/SOCIAL/SQUID_PG_CONNECTION_STRING to enable)"
        ),
    }

    Ok(Arc::new(AppStateInner {
        notifications: NotificationsComponent::new(pool.clone(), cfg.email.clone()),
        admin_token: cfg.admin_token.clone(),
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route(
            "/notifications",
            get(handlers::notifications::get_notifications),
        )
        .route(
            "/notifications/read",
            put(handlers::notifications::put_read),
        )
        .route(
            "/subscription",
            get(handlers::subscription::get_subscription)
                .put(handlers::subscription::put_subscription),
        )
        .route("/set-email", put(handlers::subscription::put_set_email))
        .route(
            "/confirm-email",
            put(handlers::subscription::put_confirm_email),
        )
        .route(
            "/subscription/opt-outs",
            post(handlers::subscription::post_opt_out),
        )
        .route(
            "/subscription/opt-outs/community/{communityId}",
            get(handlers::subscription::get_community_opt_out)
                .delete(handlers::subscription::delete_community_opt_out),
        )
        .route(
            "/notifications/broadcast",
            post(handlers::admin::post_broadcast),
        )
        .layer(axum::extract::DefaultBodyLimit::max(256 * 1024))
}
