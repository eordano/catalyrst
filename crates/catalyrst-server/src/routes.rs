use std::sync::Arc;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, State};
use axum::routing::{get, post};
use axum::Router;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::admin;
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

async fn read_only_gate(
    State(state): State<Arc<AppState>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if state.is_read_only() {
        return (
            axum::http::StatusCode::FORBIDDEN,
            "Content Server is in read-only mode; deployments are disabled",
        )
            .into_response();
    }
    next.run(request).await
}

fn content_routes(state: &Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/challenge", get(handlers::get_challenge::get_challenge))
        .route(
            "/entities/{type}",
            get(handlers::get_entities::get_entities),
        )
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
            get(handlers::get_content::get_content).head(handlers::get_content::get_content),
        )
        .route(
            "/available-content",
            get(handlers::get_available_content::get_available_content),
        )
        .route(
            "/audit/{type}/{entityId}",
            get(handlers::get_audit::get_audit),
        )
        .route(
            "/deployments",
            get(handlers::get_deployments::get_deployments),
        )
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
        )
        .route(
            "/entities",
            post(handlers::create_entity::create_entity_multipart)
                .layer(DefaultBodyLimit::max(max_deployment_size_bytes()))
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    read_only_gate,
                )),
        )
        .route(
            "/scenes/{coord}",
            axum::routing::delete(handlers::unpublish_scene::unpublish_scene).route_layer(
                axum::middleware::from_fn_with_state(state.clone(), read_only_gate),
            ),
        )
}

fn lambdas_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/lambdas/profiles",
            post(handlers::lambdas::profiles).layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/lambdas/profiles/{id}",
            get(handlers::lambdas::profile_by_id),
        )
        .route(
            "/lambdas/profile/{id}",
            get(handlers::lambdas::profile_alias),
        )
        .route(
            "/lambdas/collections/contents/{pointer}/thumbnail",
            get(handlers::get_entity_thumbnail::get_entity_thumbnail)
                .head(handlers::get_entity_thumbnail::get_entity_thumbnail),
        )
        .route(
            "/lambdas/collections/contents/{pointer}/image",
            get(handlers::get_entity_image::get_entity_image)
                .head(handlers::get_entity_image::get_entity_image),
        )
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
        .route(
            "/lambdas/crypto/validate-signature",
            post(handlers::lambdas_crypto::validate_signature),
        )
        .route(
            "/lambdas/contracts/servers",
            get(handlers::lambdas_contracts::contracts_servers),
        )
        .route(
            "/lambdas/contracts/pois",
            get(handlers::lambdas_contracts::contracts_pois),
        )
        .route(
            "/lambdas/contracts/denylisted-names",
            get(handlers::lambdas_contracts::contracts_denylisted_names),
        )
        .route(
            "/lambdas/third-party-integrations",
            get(handlers::lambdas_contracts::third_party_integrations),
        )
        .route(
            "/lambdas/users/{address}/wearables",
            get(handlers::lambdas_user_items::user_wearables),
        )
        .route(
            "/lambdas/users/{address}/emotes",
            get(handlers::lambdas_user_items::user_emotes),
        )
        .route(
            "/lambdas/users/{address}/third-party-wearables",
            get(handlers::lambdas_user_items::user_third_party_wearables),
        )
        .route(
            "/lambdas/users/{address}/third-party-wearables/{collectionId}",
            get(handlers::lambdas_user_items::user_third_party_collection_wearables),
        )
        .route(
            "/lambdas/users/{address}/names",
            get(handlers::lambdas_land::user_names),
        )
        .route(
            "/lambdas/users/{address}/lands",
            get(handlers::lambdas_land::user_lands),
        )
        .route(
            "/lambdas/users/{address}/lands-permissions",
            get(handlers::lambdas_land::user_lands_permissions),
        )
        .route(
            "/lambdas/users/{address}/parcels/{x}/{y}/permissions",
            get(handlers::lambdas_land::parcel_permissions),
        )
        .route(
            "/lambdas/names/{name}/owner",
            get(handlers::lambdas_land::name_owner),
        )
        .route(
            "/lambdas/parcels/{x}/{y}/operators",
            get(handlers::lambdas_land::parcel_operators),
        )
        .route(
            "/lambdas/explorer/{address}/wearables",
            get(handlers::lambdas_explorer::explorer_wearables),
        )
        .route(
            "/lambdas/explorer/{address}/emotes",
            get(handlers::lambdas_explorer::explorer_emotes),
        )
        .route(
            "/lambdas/nfts/collections",
            get(handlers::lambdas_catalog::nfts_collections),
        )
        .route(
            "/lambdas/outfits/{id}",
            get(handlers::lambdas_catalog::outfits),
        )
        .route(
            "/explorer/{address}/wearables",
            get(handlers::lambdas_explorer::explorer_wearables),
        )
        .route(
            "/explorer/{address}/emotes",
            get(handlers::lambdas_explorer::explorer_emotes),
        )
        .route("/outfits/{id}", get(handlers::lambdas_catalog::outfits))
}

pub fn build_router(state: Arc<AppState>) -> Router {
    admin::audit::set_global_pool(state.audit_pool.clone());

    let top: Router<Arc<AppState>> = Router::new()
        .route("/", get(handlers::console::index))
        .route("/admin", get(handlers::console::admin))
        .route("/admin/{service}", get(handlers::console::admin_service))
        .route("/admin/auth/nonce", get(admin::auth::nonce))
        .route("/admin/auth/verify", post(admin::auth::verify))
        .route("/admin/auth/logout", post(admin::auth::logout))
        .route("/admin/auth/me", get(admin::auth::me))
        .route(
            "/admin/api/content/flush-cache",
            post(admin::api::flush_deployments_cache),
        )
        .route(
            "/admin/api/content/failed-deployments/retry",
            post(admin::api::content_retry_failed),
        )
        .route(
            "/admin/api/content/failed-deployments/clear",
            post(admin::api::content_clear_failed),
        )
        .route(
            "/admin/api/content/denylist/add",
            post(admin::api::content_denylist_add),
        )
        .route(
            "/admin/api/content/denylist/remove",
            post(admin::api::content_denylist_remove),
        )
        .route(
            "/admin/api/content/denylist/list",
            post(admin::api::content_denylist_list),
        )
        .route(
            "/admin/api/content/snapshots/regenerate",
            post(admin::api::content_snapshots_regenerate),
        )
        .route(
            "/admin/api/content/challenge/refresh",
            post(admin::api::content_challenge_refresh),
        )
        .route(
            "/admin/api/content/sync/pause",
            post(admin::api::content_sync_pause),
        )
        .route(
            "/admin/api/content/sync/resume",
            post(admin::api::content_sync_resume),
        )
        .route(
            "/admin/api/content/sync/force",
            post(admin::api::content_sync_force),
        )
        .route(
            "/admin/api/content/read-only",
            post(admin::api::content_read_only),
        )
        .route(
            "/admin/api/content/accepting-users",
            post(admin::api::content_accepting_users),
        )
        .route(
            "/admin/api/telemetry/issue-state",
            post(admin::api::telemetry_issue_state),
        )
        .route("/admin/api/telemetry/sql", post(admin::api::telemetry_sql))
        .route(
            "/admin/api/create/registry-reingest",
            post(admin::api::create_registry_reingest),
        )
        .route(
            "/admin/api/create/flush-ab-cache",
            post(admin::api::create_flush_ab_cache),
        )
        .route(
            "/admin/api/social/user-ban",
            post(admin::api::social_user_ban),
        )
        .route(
            "/admin/api/social/user-unban",
            post(admin::api::social_user_unban),
        )
        .route(
            "/admin/api/social/user-warning",
            post(admin::api::social_user_warning),
        )
        .route("/admin/api/scene/reload", post(admin::api::scene_reload))
        .route(
            "/admin/api/places/reports",
            post(admin::api::places_reports_list),
        )
        .route(
            "/admin/api/places/report-resolve",
            post(admin::api::places_report_resolve),
        )
        .route(
            "/admin/api/places/place-disable",
            post(admin::api::places_place_disable),
        )
        .route(
            "/admin/api/places/pois-list",
            post(admin::api::places_pois_list),
        )
        .route(
            "/admin/api/places/poi-create",
            post(admin::api::places_poi_create),
        )
        .route(
            "/admin/api/places/poi-update",
            post(admin::api::places_poi_update),
        )
        .route(
            "/admin/api/places/poi-delete",
            post(admin::api::places_poi_delete),
        )
        .route(
            "/admin/api/places/place-highlight",
            post(admin::api::places_place_highlight),
        )
        .route(
            "/admin/api/places/place-rating",
            post(admin::api::places_place_rating),
        )
        .route(
            "/admin/api/places/world-highlight",
            post(admin::api::places_world_highlight),
        )
        .route(
            "/admin/api/places/world-rating",
            post(admin::api::places_world_rating),
        )
        .route("/admin/api/events/create", post(admin::api::events_create))
        .route(
            "/admin/api/events/moderate",
            post(admin::api::events_moderate),
        )
        .route("/admin/api/worlds/list", post(admin::api::worlds_list))
        .route("/admin/api/worlds/detail", post(admin::api::worlds_detail))
        .route("/admin/api/worlds/enable", post(admin::api::worlds_enable))
        .route(
            "/admin/api/worlds/disable",
            post(admin::api::worlds_disable),
        )
        .route(
            "/admin/api/worlds/ban-status",
            post(admin::api::worlds_ban_status),
        )
        .route(
            "/admin/api/worlds/blocked-list",
            post(admin::api::worlds_blocked_list),
        )
        .route(
            "/admin/api/worlds/blocked-add",
            post(admin::api::worlds_blocked_add),
        )
        .route(
            "/admin/api/worlds/blocked-remove",
            post(admin::api::worlds_blocked_remove),
        )
        .route(
            "/admin/api/worlds/access-log",
            post(admin::api::worlds_access_log),
        )
        .route(
            "/admin/api/create/queues-retry",
            post(admin::api::create_queues_retry),
        )
        .route(
            "/admin/api/create/queues-pause",
            post(admin::api::create_queues_pause),
        )
        .route(
            "/admin/api/create/queues-resume",
            post(admin::api::create_queues_resume),
        )
        .route(
            "/admin/api/create/queues-status",
            post(admin::api::create_queues_status),
        )
        .route(
            "/admin/api/create/denylist-add",
            post(admin::api::create_denylist_add),
        )
        .route(
            "/admin/api/create/denylist-remove",
            post(admin::api::create_denylist_remove),
        )
        .route(
            "/admin/api/camera-reel/image-delete",
            post(admin::api::camera_reel_image_delete),
        )
        .route(
            "/admin/api/camera-reel/image-review",
            post(admin::api::camera_reel_image_review),
        )
        .route(
            "/admin/api/builder/item-status",
            post(admin::api::builder_item_status),
        )
        .route(
            "/admin/api/builder/items-status",
            post(admin::api::builder_items_status_bulk),
        )
        .route(
            "/admin/api/communities/list",
            post(admin::api::communities_list),
        )
        .route(
            "/admin/api/communities/suspend",
            post(admin::api::communities_suspend),
        )
        .route(
            "/admin/api/communities/unsuspend",
            post(admin::api::communities_unsuspend),
        )
        .route(
            "/admin/api/notifications/broadcast",
            post(admin::api::notifications_broadcast),
        )
        .route("/admin/api/badges/grant", post(admin::api::badges_grant))
        .route("/admin/api/badges/revoke", post(admin::api::badges_revoke))
        .route(
            "/admin/api/social-rpc/presence",
            post(admin::api::social_rpc_presence),
        )
        .route(
            "/admin/api/social-rpc/voice-calls",
            post(admin::api::social_rpc_voice_calls),
        )
        .route(
            "/admin/api/social-rpc/friendships",
            post(admin::api::social_rpc_friendships),
        )
        .route(
            "/admin/api/social-rpc/disconnect",
            post(admin::api::social_rpc_disconnect),
        )
        .route(
            "/admin/api/social-rpc/force-presence",
            post(admin::api::social_rpc_force_presence),
        )
        .route(
            "/admin/api/social-rpc/reset-settings",
            post(admin::api::social_rpc_reset_settings),
        )
        .route(
            "/admin/api/scene-state/crdt",
            post(admin::api::scene_state_crdt),
        )
        .route(
            "/admin/api/scene-state/kick-all",
            post(admin::api::scene_state_kick_all),
        )
        .route(
            "/admin/api/scene-state/reset",
            post(admin::api::scene_state_reset),
        )
        .route(
            "/admin/api/credits/seasons-list",
            post(admin::api::credits_seasons_list),
        )
        .route(
            "/admin/api/credits/season-create",
            post(admin::api::credits_season_create),
        )
        .route(
            "/admin/api/credits/season-update",
            post(admin::api::credits_season_update),
        )
        .route(
            "/admin/api/credits/season-delete",
            post(admin::api::credits_season_delete),
        )
        .route(
            "/admin/api/credits/goals-list",
            post(admin::api::credits_goals_list),
        )
        .route(
            "/admin/api/credits/goal-create",
            post(admin::api::credits_goal_create),
        )
        .route(
            "/admin/api/credits/goal-update",
            post(admin::api::credits_goal_update),
        )
        .route(
            "/admin/api/credits/goal-delete",
            post(admin::api::credits_goal_delete),
        )
        .route("/admin/api/credits/grant", post(admin::api::credits_grant))
        .route(
            "/admin/api/credits/revoke",
            post(admin::api::credits_revoke),
        )
        .route(
            "/admin/api/credits/user-block",
            post(admin::api::credits_user_block),
        )
        .route(
            "/admin/api/price/override-set",
            post(admin::api::price_override_set),
        )
        .route(
            "/admin/api/price/override-delete",
            post(admin::api::price_override_delete),
        )
        .route("/admin/api/rpc/config", post(admin::api::rpc_config))
        .route(
            "/admin/api/rpc/methods-list",
            post(admin::api::rpc_methods_list),
        )
        .route(
            "/admin/api/rpc/methods-add",
            post(admin::api::rpc_methods_add),
        )
        .route(
            "/admin/api/rpc/methods-remove",
            post(admin::api::rpc_methods_remove),
        )
        .route(
            "/admin/api/rpc/methods-reset",
            post(admin::api::rpc_methods_reset),
        )
        .route(
            "/admin/api/rpc/networks-list",
            post(admin::api::rpc_networks_list),
        )
        .route(
            "/admin/api/rpc/networks-set",
            post(admin::api::rpc_networks_set),
        )
        .route(
            "/admin/api/rpc/networks-delete",
            post(admin::api::rpc_networks_delete),
        )
        .route(
            "/admin/api/explorer-api/flags-toggle",
            post(admin::api::explorer_api_flags_toggle),
        )
        .route(
            "/admin/api/explorer-api/flags-reload",
            post(admin::api::explorer_api_flags_reload),
        )
        .route(
            "/admin/api/explorer-api/blocklist-add",
            post(admin::api::explorer_api_blocklist_add),
        )
        .route(
            "/admin/api/explorer-api/blocklist-remove",
            post(admin::api::explorer_api_blocklist_remove),
        )
        .route(
            "/admin/api/explorer-api/blocklist-reload",
            post(admin::api::explorer_api_blocklist_reload),
        )
        .route(
            "/admin/api/explorer-api/config-list",
            post(admin::api::explorer_api_config_list),
        )
        .route(
            "/admin/api/explorer-api/config-get",
            post(admin::api::explorer_api_config_get),
        )
        .route(
            "/admin/api/explorer-api/config-set",
            post(admin::api::explorer_api_config_set),
        )
        .route(
            "/admin/api/explorer-api/config-delete",
            post(admin::api::explorer_api_config_delete),
        )
        .route(
            "/admin/api/explorer-api/challenges-list",
            post(admin::api::explorer_api_challenges_list),
        )
        .route(
            "/admin/api/explorer-api/challenge-get",
            post(admin::api::explorer_api_challenge_get),
        )
        .route(
            "/admin/api/explorer-api/challenge-revoke",
            post(admin::api::explorer_api_challenge_revoke),
        )
        .route(
            "/admin/api/explorer-api/identities-list",
            post(admin::api::explorer_api_identities_list),
        )
        .route(
            "/admin/api/explorer-api/identity-revoke",
            post(admin::api::explorer_api_identity_revoke),
        )
        .route(
            "/admin/api/telemetry/purge",
            post(admin::api::telemetry_purge),
        )
        .route(
            "/admin/api/telemetry/ingest",
            post(admin::api::telemetry_ingest),
        )
        .route(
            "/admin/api/telemetry/quota",
            post(admin::api::telemetry_quota),
        )
        .route(
            "/admin/api/telemetry/bulk-delete",
            post(admin::api::telemetry_bulk_delete),
        )
        .route(
            "/admin/api/telemetry/export",
            post(admin::api::telemetry_export),
        )
        .route(
            "/admin/api/telemetry/audit",
            post(admin::api::telemetry_audit),
        )
        .route(
            "/admin/api/telemetry/regroup",
            post(admin::api::telemetry_regroup),
        )
        .route(
            "/admin/api/telemetry/release",
            post(admin::api::telemetry_release),
        )
        .route("/about", get(handlers::about::get_about));

    let mut app = top
        .merge(content_routes(&state))
        .merge(lambdas_routes())
        .nest("/content", content_routes(&state))
        .fallback(not_found)
        .route("/metrics", get(crate::metrics::metrics_handler))
        .layer(axum::middleware::from_fn(crate::metrics::track_http))
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(
            crate::nul_guard::nul_guard_middleware,
        ))
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
