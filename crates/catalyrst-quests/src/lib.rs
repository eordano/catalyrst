//! catalyrst-quests — port of `decentraland/quests` (upstream is itself Rust,
//! archived under the mirrors).
//!
//! - REST surface (list/get/reward/by-creator/instances/state) serves the
//!   protobuf-defined message shapes (`decentraland.quests` definitions.proto
//!   via prost, camelCase serde) byte-compatibly with upstream, mounted under
//!   `/api`.
//! - The dcl-rpc `QuestsService` (StartQuest/AbortQuest/SendEvent/Subscribe/
//!   GetAllQuests/GetQuestDefinition) is served over a signed-auth-chain
//!   WebSocket transport at `/ws`, with an in-process event processor driving
//!   `apply_event`, reward hooks, completion, and per-user `UserUpdate` fan-out.

pub mod auth_chain;
pub mod config;
pub mod context;
pub mod db;
pub mod handlers;
pub mod processor;
pub mod proto;
pub mod quests;
pub mod rpc;
pub mod service;
pub mod state;
pub mod transport;

use std::sync::Arc;

use axum::routing::{any, get};
use axum::Router;

pub use db::Db;
use rpc::RpcRuntime;

/// HTTP/REST state: the (optional) DB pool, mirrored by the RPC runtime.
#[derive(Clone)]
pub struct AppState {
    pub db: Option<Arc<Db>>,
}

/// Build the full quests router: REST under `/api`, the dcl-rpc WS transport at
/// `/ws`, and a liveness probe. When a DB is present the RPC server + event
/// processor are started and the `/ws` route is mounted.
pub async fn build_router(db: Option<Arc<Db>>) -> Router {
    let rest = Router::new()
        .route("/api/quests", get(handlers::get_quests))
        .route("/api/quests/{quest_id}", get(handlers::get_quest))
        .route(
            "/api/quests/{quest_id}/reward",
            get(handlers::get_quest_reward),
        )
        .route(
            "/api/quests/{quest_id}/instances",
            get(handlers::get_quest_instances),
        )
        .route(
            "/api/creators/{user_address}/quests",
            get(handlers::get_quests_by_creator),
        )
        .route(
            "/api/instances/{quest_instance}/state",
            get(handlers::get_instance_state),
        )
        .with_state(AppState { db: db.clone() });

    let mut router = Router::new()
        .route("/health/live", get(handlers::health))
        .merge(rest);

    if let Some(db) = db {
        // Wire the in-process context + event processor + dcl-rpc server. The
        // WS route carries its own `Arc<RpcRuntime>` state, fully resolved to a
        // `Router<()>` before merging into the main router.
        let (ctx, events_rx) = context::Context::new(db);
        processor::spawn_event_processor(ctx.clone(), events_rx);
        let runtime = RpcRuntime::new(ctx, config::auth_window_secs());
        runtime.init().await;
        let ws = Router::new()
            .route("/ws", any(rpc::ws_upgrade))
            .with_state(runtime);
        router = router.merge(ws);
    }

    router
}
