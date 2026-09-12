#![allow(clippy::result_large_err)]

pub mod access;
pub mod admin;
pub mod auth_chain;
pub mod config;
pub mod contents_temp;
pub mod fed;
pub mod handlers;
pub mod http;
pub mod livekit;
pub mod ports;
pub mod settings_policy;
pub mod upload_limits;
pub mod world_storage;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::get;
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use utoipa::OpenApi;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

use crate::config::Config;
use crate::ports::bans::BansComponent;
use crate::ports::denylist::DenyListComponent;
use crate::ports::name_denylist::NameDenyListChecker;
use crate::ports::presence::PeersRegistry;
use crate::ports::worlds::WorldsComponent;
use catalyrst_fed::RateLimiter;

pub const RATE_LIMIT_MAX_ATTEMPTS: u32 = 3;
pub const RATE_LIMIT_WINDOW_SECONDS: u64 = 60;

pub struct AppStateInner {
    pub cfg: Config,
    pub worlds: WorldsComponent,
    pub presence: PeersRegistry,
    pub rate_limiter: RateLimiter,
    pub bans: BansComponent,
    pub denylist: DenyListComponent,
    pub name_denylist: NameDenyListChecker,
    pub http: reqwest::Client,
    pub squid_pool: Option<sqlx::PgPool>,
    /// Liveness of the SFU this realm advertises. `/about` serves
    /// `offline:offline` while it is down, because a comms endpoint that does
    /// not answer blocks entry outright rather than degrading it.
    pub sfu: catalyrst_livekit::SfuHealth,
    /// The worlds-federation peer set, fixed for the lifetime of this process and
    /// resolved in [`build_state`] *before* this struct is built, so a bad
    /// `federation-peers.toml` aborts startup rather than degrading a serving process.
    ///
    /// A peer here is a source of content claims and nothing else: no ownership or
    /// permission question resolves through this field: those go through
    /// [`handlers::permissions::resolve_world_owner`].
    pub fed_peers: fed::peers::WorldsFederationPeers,
    /// The read mirror: `remote_worlds` + `remote_peer_status`, and the poller that
    /// fills them.
    ///
    /// Deliberately **not** part of [`ports::worlds::WorldsComponent`]: were mirrored
    /// rows in that struct, `state.worlds.get_world(name)` could reach them and five
    /// separate owner comparisons would each have to remember not to. No method here
    /// returns a [`ports::worlds::WorldRecord`] or writes a column of `worlds` /
    /// `world_scenes`, and the rows it returns have no `owner` field at all.
    pub mirror: fed::poll::WorldsMirror,
}

pub type AppState = Arc<AppStateInner>;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub async fn build_state(cfg: Config) -> Result<AppState> {
    let fed_peers = fed::peers::WorldsFederationPeers::load_from_env()?;

    let pool =
        catalyrst_db::connect_pool(&cfg.database_url, &catalyrst_db::PoolSettings::default())
            .await
            .context("failed to connect worlds pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run worlds migrations")?;

    let squid_pool = match cfg.squid_database_url.as_deref() {
        Some(url) => {
            let opts = PgConnectOptions::from_str(url)
                .context("invalid SQUID_PG_CONNECTION_STRING")?
                .options([("statement_timeout", "15000")]);
            match PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .idle_timeout(Duration::from_secs(60))
                .connect_with(opts)
                .await
            {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to connect squid marketplace pool; NAME-ownership publish authz disabled (fail-closed \u{2192} deny)");
                    None
                }
            }
        }
        None => {
            tracing::warn!("SQUID_PG_CONNECTION_STRING unset; NAME-ownership publish authz disabled (fail-closed \u{2192} deny)");
            None
        }
    };

    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .context("build http client")?;
    let bans = BansComponent::new(
        http.clone(),
        cfg.comms_gatekeeper_url.clone(),
        cfg.comms_gatekeeper_auth_token.clone(),
    );
    let denylist = DenyListComponent::new(http.clone(), cfg.denylist_json_url.clone());
    let name_denylist = NameDenyListChecker::new(http.clone(), cfg.dcl_lists_url.clone());

    contents_temp::spawn_reaper(
        cfg.contents_dir.clone(),
        contents_temp::reap_grace(
            cfg.multipart_upload_timeout_ms,
            cfg.deployment_processing_timeout_ms,
        ),
    );

    let sfu = match cfg.comms_offline_when_unreachable {
        true => match catalyrst_livekit::probe_target(&cfg.livekit_ws_url) {
            Some(target) => catalyrst_livekit::SfuHealth::spawn(target),
            None => catalyrst_livekit::SfuHealth::always_alive(),
        },
        false => catalyrst_livekit::SfuHealth::always_alive(),
    };

    let mirror = fed::poll::WorldsMirror::new(pool.clone(), cfg.federation.clone(), &fed_peers);

    mirror
        .store()
        .revoke_peers_no_longer_admitted(&fed_peers)
        .await
        .context(
            "failed to reconcile the worlds mirror against the admitted peer set; refusing to \
             start rather than serve worlds for a peer that may have been de-admitted",
        )?;

    let state = Arc::new(AppStateInner {
        worlds: WorldsComponent::new(pool),
        presence: PeersRegistry::new(),
        rate_limiter: RateLimiter::new(
            RATE_LIMIT_MAX_ATTEMPTS,
            Duration::from_secs(RATE_LIMIT_WINDOW_SECONDS),
        ),
        bans,
        denylist,
        name_denylist,
        http,
        squid_pool,
        sfu,
        fed_peers,
        mirror,
        cfg,
    });

    fed::poll::spawn_poller(state.clone());

    Ok(state)
}

#[derive(OpenApi)]
#[openapi(info(title = "catalyrst-worlds"))]
struct ApiDoc;

pub fn api_router_with_spec() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    build_api(true)
}

pub fn api_router_with_spec_without_status() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    build_api(false)
}

fn build_api(include_status: bool) -> (Router<AppState>, utoipa::openapi::OpenApi) {
    let base = OpenApiRouter::with_openapi(ApiDoc::openapi());
    let base = if include_status {
        base.routes(routes!(handlers::status::status))
    } else {
        base
    };
    base.routes(routes!(handlers::index::get_index))
        .routes(routes!(handlers::about::get_about))
        .routes(routes!(handlers::about::get_server_about))
        .routes(routes!(handlers::worlds_list::get_worlds))
        .routes(routes!(handlers::world_manifest::get_world_manifest))
        .routes(routes!(handlers::preview_wearables::get_preview_wearables))
        .routes(routes!(handlers::permissions::get_permissions))
        .routes(routes!(handlers::permissions::post_permissions))
        .routes(routes!(
            handlers::permissions::get_allowed_parcels_for_permission,
            handlers::permissions::post_permission_parcels,
            handlers::permissions::delete_permission_parcels
        ))
        .routes(routes!(
            handlers::permissions::get_addresses_for_parcel_permission
        ))
        .routes(routes!(
            handlers::permissions::put_permissions_access_community,
            handlers::permissions::delete_permissions_access_community
        ))
        .routes(routes!(
            handlers::permissions::put_permissions_address,
            handlers::permissions::delete_permissions_address
        ))
        .routes(routes!(handlers::active::active_entities))
        .routes(routes!(handlers::scenes::undeploy_world))
        .routes(routes!(handlers::scenes::list_scenes))
        .routes(routes!(handlers::scenes::delete_scene))
        .routes(routes!(handlers::comms::world_comms))
        .routes(routes!(handlers::comms::world_scene_comms))
        .routes(routes!(handlers::comms::get_comms_adapter))
        .routes(routes!(
            handlers::contents::get_content,
            handlers::contents::head_content
        ))
        .routes(routes!(
            handlers::contents::get_ipfs,
            handlers::contents::head_ipfs
        ))
        .routes(routes!(handlers::contents::available_content))
        .routes(routes!(handlers::wallet::connected_world))
        .routes(routes!(handlers::live_data::live_data))
        .routes(routes!(handlers::webhook::livekit_webhook))
        .routes(routes!(handlers::admin::list_worlds))
        .routes(routes!(handlers::admin::world_detail))
        .routes(routes!(handlers::admin::enable_world))
        .routes(routes!(handlers::admin::disable_world))
        .routes(routes!(handlers::admin::world_ban_status))
        .routes(routes!(handlers::admin::list_blocked))
        .routes(routes!(
            handlers::admin::block_wallet,
            handlers::admin::unblock_wallet
        ))
        .routes(routes!(handlers::admin::access_log))
        .routes(routes!(handlers::gc::garbage_collect))
        .routes(routes!(handlers::gc::garbage_collect_root))
        .routes(routes!(fed::handlers::get_federation_peers))
        .routes(routes!(fed::handlers::get_federation_mirror))
        .routes(routes!(fed::handlers::refresh_federation_mirror))
        .routes(routes!(fed::handlers::set_mirror_world_hidden))
        .routes(routes!(handlers::world_settings::get_world_settings))
        .layer(catalyrst_envcfg::service_scaffold::request_timeout_layer(
            30,
        ))
        .routes(
            routes!(handlers::world_settings::update_world_settings).layer(
                axum::extract::DefaultBodyLimit::max(
                    handlers::world_settings::MAX_SETTINGS_UPLOAD_WIRE_BYTES,
                ),
            ),
        )
        .routes(routes!(handlers::deploy::deploy_entity).layer(
            axum::extract::DefaultBodyLimit::max(handlers::deploy::MAX_UPLOAD_WIRE_SIZE_BYTES),
        ))
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
            "worlds",
            &serde_json::from_str(&rendered).expect("spec round-trips through JSON"),
        );
        let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") else {
            return;
        };
        let out = std::path::Path::new(&dir).join("openapi");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("worlds.openapi.json"), rendered).unwrap();
    }
}
