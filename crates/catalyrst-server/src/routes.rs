use std::sync::Arc;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::state::AppState;

const DEFAULT_MAX_DEPLOYMENT_SIZE_BYTES: usize = 200 * 1024 * 1024;

fn max_deployment_size_bytes() -> usize {
    std::env::var("MAX_DEPLOYMENT_SIZE_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_DEPLOYMENT_SIZE_BYTES)
}

const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 60;

fn request_timeout() -> Option<Duration> {
    match std::env::var("REQUEST_TIMEOUT_SECS") {
        Ok(v) => match v.parse::<u64>() {
            Ok(0) => None,
            Ok(n) => Some(Duration::from_secs(n)),
            Err(_) => Some(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS)),
        },
        Err(_) => Some(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS)),
    }
}

async fn not_found() -> impl axum::response::IntoResponse {
    (axum::http::StatusCode::NOT_FOUND, "Not found")
}

fn content_routes(read_only: bool) -> Router<Arc<AppState>> {
    let mut r: Router<Arc<AppState>> = Router::new()
        .route("/challenge", get(handlers::get_challenge::get_challenge))
        .route("/entities/{type}", get(handlers::get_entities::get_entities))
        .route(
            "/entities/active/collections/{collectionUrn}",
            get(handlers::filter_by_urn::get_entities_by_collection),
        )
        .route(
            "/entities/active",
            post(handlers::active_entities::get_active_entities)
                .layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/contents/{hashId}",
            get(handlers::get_content::get_content)
                .head(handlers::get_content::get_content),
        )
        .route(
            "/available-content",
            get(handlers::get_available_content::get_available_content),
        )
        .route("/audit/{type}/{entityId}", get(handlers::get_audit::get_audit))
        .route("/deployments", get(handlers::get_deployments::get_deployments))
        .route(
            "/contents/{hashId}/active-entities",
            get(handlers::get_active_entities_by_hash::get_active_entities_by_hash),
        )
        .route(
            "/failed-deployments",
            get(handlers::failed_deployments::get_failed_deployments),
        )
        .route(
            "/pointer-changes",
            get(handlers::pointer_changes::get_pointer_changes),
        )
        .route("/snapshots", get(handlers::get_snapshots::get_snapshots))
        .route("/status", get(handlers::status::get_status))
        .route(
            "/queries/items/{pointer}/thumbnail",
            get(handlers::get_entity_thumbnail::get_entity_thumbnail)
                .head(handlers::get_entity_thumbnail::get_entity_thumbnail),
        )
        .route(
            "/queries/items/{pointer}/image",
            get(handlers::get_entity_image::get_entity_image)
                .head(handlers::get_entity_image::get_entity_image),
        )
        .route(
            "/queries/erc721/{chainId}/{contract}/{option}",
            get(handlers::get_erc721_entity::get_erc721_entity),
        )
        .route(
            "/queries/erc721/{chainId}/{contract}/{option}/{emission}",
            get(handlers::get_erc721_entity::get_erc721_entity),
        );
    if !read_only {
        r = r.route(
            "/entities",
            post(handlers::create_entity::create_entity_multipart)
                .layer(DefaultBodyLimit::max(max_deployment_size_bytes())),
        );
    }
    r
}

fn lambdas_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/lambdas/profiles",
            post(handlers::lambdas::profiles)
                .layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route("/lambdas/profiles/{id}", get(handlers::lambdas::profile_by_id))
        .route("/lambdas/profile/{id}", get(handlers::lambdas::profile_alias))
        .route(
            "/lambdas/collections/wearables",
            get(handlers::lambdas_catalog::collections_wearables_catalog),
        )
        .route(
            "/lambdas/collections/emotes",
            get(handlers::lambdas_catalog::collections_emotes_catalog),
        )
        .route(
            "/lambdas/collections/wearables-by-owner/{owner}",
            get(handlers::lambdas::wearables_by_owner),
        )
        .route(
            "/lambdas/collections/emotes-by-owner/{owner}",
            get(handlers::lambdas::emotes_by_owner),
        )
        .route("/lambdas/status", get(handlers::lambdas::lambdas_status))
        .route("/lambdas/contracts/servers", get(handlers::lambdas_contracts::contracts_servers))
        .route("/lambdas/contracts/pois", get(handlers::lambdas_contracts::contracts_pois))
        .route("/lambdas/contracts/denylisted-names", get(handlers::lambdas_contracts::contracts_denylisted_names))
        .route("/lambdas/third-party-integrations", get(handlers::lambdas_contracts::third_party_integrations))
        .route("/lambdas/users/{address}/wearables", get(handlers::lambdas_user_items::user_wearables))
        .route("/lambdas/users/{address}/emotes", get(handlers::lambdas_user_items::user_emotes))
        .route("/lambdas/users/{address}/third-party-wearables", get(handlers::lambdas_user_items::user_third_party_wearables))
        .route("/lambdas/users/{address}/third-party-wearables/{collectionId}", get(handlers::lambdas_user_items::user_third_party_collection_wearables))
        .route("/lambdas/users/{address}/names", get(handlers::lambdas_land::user_names))
        .route("/lambdas/users/{address}/lands", get(handlers::lambdas_land::user_lands))
        .route("/lambdas/users/{address}/lands-permissions", get(handlers::lambdas_land::user_lands_permissions))
        .route("/lambdas/users/{address}/parcels/{x}/{y}/permissions", get(handlers::lambdas_land::parcel_permissions))
        .route("/lambdas/names/{name}/owner", get(handlers::lambdas_land::name_owner))
        .route("/lambdas/parcels/{x}/{y}/operators", get(handlers::lambdas_land::parcel_operators))
        .route("/lambdas/explorer/{address}/wearables", get(handlers::lambdas_explorer::explorer_wearables))
        .route("/lambdas/explorer/{address}/emotes", get(handlers::lambdas_explorer::explorer_emotes))
        .route("/lambdas/nfts/collections", get(handlers::lambdas_catalog::nfts_collections))
        .route("/lambdas/outfits/{id}", get(handlers::lambdas_catalog::outfits))
        // Root-level aliases: the unity client builds these against the bare realm
        // host (its lambdas base loses the /lambdas path on URL join), so the
        // explorer/backpack endpoints must also answer without the prefix.
        .route("/explorer/{address}/wearables", get(handlers::lambdas_explorer::explorer_wearables))
        .route("/explorer/{address}/emotes", get(handlers::lambdas_explorer::explorer_emotes))
        .route("/outfits/{id}", get(handlers::lambdas_catalog::outfits))
}

pub fn build_router(state: Arc<AppState>) -> Router {
    let read_only = state.read_only;

    let top: Router<Arc<AppState>> = Router::new()
        .route("/about", get(handlers::about::get_about));

    let mut app = top
        .merge(content_routes(read_only))
        .merge(lambdas_routes())
        .nest("/content", content_routes(read_only))
        .fallback(not_found)
        .route("/metrics", get(crate::metrics::metrics_handler))
        .layer(axum::middleware::from_fn(crate::metrics::track_http))
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(crate::cors::cors_middleware));

    if let Some(timeout) = request_timeout() {
        tracing::info!(?timeout, "request timeout enabled");
        app = app.layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            timeout,
        ));
    }

    app.with_state(state)
}
