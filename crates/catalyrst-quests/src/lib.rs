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
pub mod validation;

use std::sync::Arc;

use axum::routing::{any, delete, get, patch, post, put};
use axum::Router;

use context::Context;
pub use db::Db;
use rpc::RpcRuntime;

#[derive(Clone)]
pub struct AppState {
    pub db: Option<Arc<Db>>,
    pub ctx: Option<Context>,
}

pub async fn build_router(db: Option<Arc<Db>>) -> Router {
    let mut ctx_opt = None;
    let mut ws_router = None;

    if let Some(db) = &db {
        let (ctx, events_rx) = Context::new(db.clone());
        processor::spawn_event_processor(ctx.clone(), events_rx);
        let runtime = RpcRuntime::new(ctx.clone(), config::auth_window_secs());
        runtime.init().await;
        ctx_opt = Some(ctx);
        ws_router = Some(
            Router::new()
                .route("/ws", any(rpc::ws_upgrade))
                .with_state(runtime),
        );
    }

    let rest = Router::new()
        .route(
            "/api/quests",
            get(handlers::get_quests).post(handlers::authoring::create_quest),
        )
        .route(
            "/api/quests/{quest_id}",
            get(handlers::get_quest)
                .put(handlers::authoring::update_quest)
                .delete(handlers::authoring::delete_quest),
        )
        .route(
            "/api/quests/{quest_id}/activate",
            put(handlers::authoring::activate_quest),
        )
        .route(
            "/api/quests/{quest_id}/stats",
            get(handlers::authoring::get_quest_stats),
        )
        .route(
            "/api/quests/{quest_id}/updates",
            get(handlers::authoring::get_quest_updates),
        )
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
            "/api/instances/{quest_instance}",
            get(handlers::instances::get_quest_instance),
        )
        .route(
            "/api/instances/{quest_instance}/events",
            post(handlers::instances::add_event_to_instance),
        )
        .route(
            "/api/instances/{quest_instance}/events/{event_id}",
            delete(handlers::instances::remove_event_from_instance),
        )
        .route(
            "/api/instances/{quest_instance}/reset",
            patch(handlers::instances::reset_quest_instance),
        )
        .route(
            "/api/instances/{quest_instance}/state",
            get(handlers::get_instance_state),
        )
        .with_state(AppState {
            db: db.clone(),
            ctx: ctx_opt,
        });

    let mut router = Router::new()
        .route("/health/live", get(handlers::health))
        .merge(rest);

    if let Some(ws) = ws_router {
        router = router.merge(ws);
    }

    router
}
