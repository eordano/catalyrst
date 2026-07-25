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

#[derive(Clone)]
pub struct AppState {
    pub db: Option<Arc<Db>>,
}

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
