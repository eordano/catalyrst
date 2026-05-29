//! catalyrst-market — REST API on top of the squid_marketplace schema.
//!
//! Mirrors `decentraland/marketplace-server` (Node.js) endpoint-by-endpoint
//! and response-shape-by-response-shape so existing clients are byte-identical.
//! Reads only — write endpoints (trades, favorites, transak, wert) are out of
//! scope until the federation ADR is written.

pub mod config;
pub mod dcl_schemas;
pub mod http;
pub mod handlers;
pub mod marketplace_contracts;
pub mod ports;

use std::sync::Arc;

use crate::ports::contracts::ContractsComponent;

/// Schema name that the squid indexer writes into. Mirrors
/// `marketplace-server/src/constants.ts:MARKETPLACE_SQUID_SCHEMA`.
pub const MARKETPLACE_SQUID_SCHEMA: &str = "squid_marketplace";

/// Builder-server materialized-view schema. Mirrors
/// `marketplace-server/src/constants.ts:BUILDER_SERVER_TABLE_SCHEMA`.
pub const BUILDER_SERVER_TABLE_SCHEMA: &str = "marketplace";

/// Shared component container — mirrors `types.ts:AppComponents`.
/// Each new port gets a field here.
pub struct AppStateInner {
    pub contracts: ContractsComponent,
}

pub type AppState = Arc<AppStateInner>;
