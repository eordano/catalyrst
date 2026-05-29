// Handlers use Result<T, axum::Response> (~512B) and sqlx row tuples are wire-shaped.
#![allow(clippy::result_large_err, clippy::type_complexity)]

pub mod cache;
pub mod cors;
pub mod errors;
pub mod formatters;
pub mod handlers;
pub mod metrics;
pub mod query_params;
pub mod routes;
pub mod state;
pub mod validation;
pub mod entity_cache;
pub mod sync_backends;
pub mod third_party_refresh;
pub mod write_deployer;
