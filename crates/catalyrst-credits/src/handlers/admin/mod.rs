use axum::routing::{get, post};
use axum::Router;

use crate::AppState;

mod catalog;
mod common;
mod ops;

pub(crate) use common::validate_positive_amount;

use catalog::{
    create_goal, create_pack, create_season, delete_goal, delete_pack, delete_season, list_goals,
    list_packs, list_seasons, update_goal, update_pack, update_season,
};
use ops::{
    block_user, force_fulfill_checkout, grant_credits, list_checkouts, list_ledger, list_purchases,
    reclaim_grant, reconcile, refund_checkout, release_grant, revoke_credits,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/seasons", get(list_seasons).post(create_season))
        .route(
            "/admin/seasons/{id}",
            axum::routing::put(update_season).delete(delete_season),
        )
        .route("/admin/goals", get(list_goals).post(create_goal))
        .route(
            "/admin/goals/{id}",
            axum::routing::put(update_goal).delete(delete_goal),
        )
        .route("/admin/credits/grant", post(grant_credits))
        .route("/admin/credits/revoke", post(revoke_credits))
        .route("/admin/users/{address}/block", post(block_user))
        .route("/admin/packs", get(list_packs).post(create_pack))
        .route(
            "/admin/packs/{sku}",
            axum::routing::put(update_pack).delete(delete_pack),
        )
        .route("/admin/purchases", get(list_purchases))
        .route("/admin/checkouts", get(list_checkouts))
        .route("/admin/ledger", get(list_ledger))
        .route("/admin/checkouts/{id}/refund", post(refund_checkout))
        .route(
            "/admin/checkouts/{id}/force-fulfill",
            post(force_fulfill_checkout),
        )
        .route("/admin/grants/{escrow_ref}/reclaim", post(reclaim_grant))
        .route("/admin/grants/{escrow_ref}/release", post(release_grant))
        .route("/admin/reconcile", get(reconcile))
}
