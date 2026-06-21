pub mod auth;
pub mod auth_chain;
pub mod clients;
pub mod config;
pub mod fed;
pub mod handlers;
pub mod http;
pub mod ports;
pub mod s3;
pub mod snapshot;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, patch, post, put};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::clients::{CommsGatekeeper, Events};
use crate::config::Config;
use crate::ports::places::PlacesComponent;

pub struct AppStateInner {
    pub places: PlacesComponent,
    pub admin_addresses: Vec<String>,
    pub data_team_auth_token: Option<String>,
    pub admin_auth_token: Option<String>,
    /// comms-gatekeeper client — real-time scene/world participant lists for
    /// the `/destinations` `with_connected_users` path.
    pub comms_gatekeeper: CommsGatekeeper,
    /// Events API client — live-event status for the `/destinations`
    /// `with_live_events` path.
    pub events: Events,
    /// Federation gossip transport (places.md §4: NATS JetStream
    /// `fed.places.actions`). Defaults to the NoopPublisher; opt in with
    /// `FED_GOSSIP=nats`. A no-peer deploy applies favorites/votes locally and
    /// publishes into the void.
    pub gossip: Arc<dyn catalyrst_fed::GossipPublisher>,
    /// EIP-712 domain for place opinions (00-primitives.md §2.1).
    pub domain: catalyrst_fed::Eip712Domain,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let opts = PgConnectOptions::from_str(&cfg.places_database_url)
        .context("invalid PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING")?
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

    let mut places = PlacesComponent::new(pool);

    if let Some(writer_url) = &cfg.places_writer_database_url {
        match PgConnectOptions::from_str(writer_url) {
            Ok(writer_opts) => {
                let writer_opts = writer_opts.options([
                    ("statement_timeout", "60000"),
                    ("idle_in_transaction_session_timeout", "30000"),
                ]);
                match PgPoolOptions::new()
                    .max_connections(5)
                    .idle_timeout(Duration::from_secs(30))
                    .connect_with(writer_opts)
                    .await
                {
                    Ok(writer_pool) => {
                        places = places.with_writer(writer_pool);
                        if let Err(e) = places.ensure_local_schema().await {
                            tracing::warn!(error = %e, "could not ensure local interaction tables; favorites/likes/report writes may degrade");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "writer pool unavailable; favorites/likes/report persistence disabled (503)");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "invalid writer connection string; favorites/likes/report persistence disabled (503)");
            }
        }
    } else {
        tracing::info!(
            "no writer connection configured; favorites/likes/report persistence disabled (503)"
        );
    }

    if let Some(squid_url) = &cfg.squid_database_url {
        match PgConnectOptions::from_str(squid_url) {
            Ok(squid_opts) => {
                match PgPoolOptions::new()
                    .max_connections(5)
                    .idle_timeout(Duration::from_secs(30))
                    .connect_with(squid_opts)
                    .await
                {
                    Ok(squid_pool) => {
                        places = places.with_squid(squid_pool, cfg.squid_schema.clone());
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "squid pool unavailable; owner filter disabled");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "invalid squid connection string; owner filter disabled");
            }
        }
    }

    let gossip = catalyrst_fed::build_publisher(&catalyrst_fed::GossipConfig::from_env()).await;
    tracing::info!(
        gossip_live = gossip.is_live(),
        "places gossip publisher ready"
    );

    let state = Arc::new(AppStateInner {
        places,
        admin_addresses: cfg.admin_addresses.clone(),
        data_team_auth_token: cfg.data_team_auth_token.clone(),
        admin_auth_token: cfg.admin_auth_token.clone(),
        comms_gatekeeper: CommsGatekeeper::new(cfg.comms_gatekeeper_url.clone()),
        events: Events::new(cfg.events_api_url.clone()),
        gossip,
        domain: catalyrst_fed::sig::domains::places(),
    });

    // Spawn the federation gossip consumer apply-loop (places.md §4). No-op when
    // gossip reaches no peers (single-node default); live under FED_GOSSIP=nats.
    crate::fed::consumer::spawn(state.clone()).await;

    Ok(state)
}

pub fn api_router() -> Router<AppState> {
    let api = Router::new()
        .route("/categories", get(handlers::categories::get_categories))
        .route(
            "/places/{entity_id}/favorites",
            patch(handlers::federation::patch_place_favorites),
        )
        .route(
            "/places/{entity_id}/likes",
            patch(handlers::federation::patch_place_likes),
        )
        .route("/places/{place_id}", get(handlers::places::get_place))
        .route(
            "/places",
            get(handlers::places::get_place_list).post(handlers::places::post_place_list_by_id),
        )
        .route(
            "/places/{place_id}/rating",
            put(handlers::federation::put_place_rating),
        )
        .route(
            "/places/{place_id}/ranking",
            put(handlers::federation::put_place_ranking),
        )
        .route(
            "/places/{place_id}/highlight",
            put(handlers::federation::put_place_highlight),
        )
        .route(
            "/places/{place_id}/categories",
            get(handlers::categories::get_place_categories),
        )
        .route(
            "/places/{place_id}/featured",
            put(handlers::federation::put_place_featured)
                .delete(handlers::federation::delete_place_featured),
        )
        .route(
            "/places/status",
            post(handlers::places::post_place_status_list_by_id),
        )
        .route("/worlds/{world_id}", get(handlers::worlds::get_world))
        .route("/worlds", get(handlers::worlds::get_world_list))
        .route("/world_names", get(handlers::worlds::get_world_names_list))
        .route(
            "/worlds/{world_id}/favorites",
            patch(handlers::federation::patch_world_favorites),
        )
        .route(
            "/worlds/{world_id}/likes",
            patch(handlers::federation::patch_world_likes),
        )
        .route(
            "/worlds/{world_id}/highlight",
            put(handlers::federation::put_world_highlight),
        )
        .route(
            "/worlds/{world_id}/ranking",
            put(handlers::federation::put_world_ranking),
        )
        .route(
            "/worlds/{world_id}/rating",
            put(handlers::federation::put_world_rating),
        )
        .route(
            "/worlds/{world_id}/featured",
            put(handlers::federation::put_world_featured)
                .delete(handlers::federation::delete_world_featured),
        )
        .route("/report", post(handlers::report::post_report))
        .route(
            "/report/upload/{filename}",
            put(handlers::report::put_report_upload),
        )
        .route("/map", get(handlers::map::get_map_places))
        .route("/map/places", get(handlers::map::get_all_places_list))
        .route(
            "/destinations",
            get(handlers::destinations::get_destinations_list)
                .post(handlers::destinations::post_destinations_list_by_id),
        )
        .route("/status", get(handlers::status::status))
        // LATER admin-console controls (admin-console.md §4), admin-bearer
        // gated. Route ordering: place these specific paths before the
        // `/places/{place_id}` catch-all is unaffected (axum matches exact
        // segments first); disable lives under the place namespace.
        .route("/reports", get(handlers::admin::get_reports))
        .route("/reports/{id}", patch(handlers::admin::patch_report))
        .route(
            "/places/{place_id}/disable",
            patch(handlers::admin::patch_place_disable),
        )
        .route(
            "/pois",
            get(handlers::admin::get_pois).post(handlers::admin::post_poi),
        )
        .route(
            "/pois/{position}",
            patch(handlers::admin::patch_poi).delete(handlers::admin::delete_poi),
        );

    let social = Router::new()
        .route("/place", get(handlers::social::inject_place_metadata))
        .route("/world", get(handlers::social::inject_world_metadata));

    Router::new().nest("/api", api).nest("/places", social)
}
