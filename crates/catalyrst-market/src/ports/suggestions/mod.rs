//! `/v3/catalog/suggested`: a personalised rail over the same item-unified core the browse grid
//! serves, plus the offline job that precomputes item neighbours for it.
//!
//! The split is the point. Anything that scales with the CATALOGUE -- co-ownership over every
//! paid acquisition, content similarity over every item -- is computed by `job`, six-hourly, into
//! `marketplace.item_neighbors`. Anything that scales with ONE WALLET -- its taste profile, the
//! blend, the diversity re-rank -- runs per request in `component`. A request therefore never
//! touches the acquisition matrix, and the rail's latency is a handful of indexed lookups.

pub mod candidates;
pub mod co_ownership;
pub mod component;
pub mod constants;
pub mod content;
pub mod job;
pub mod neighbours;
pub mod profile;
pub mod queries;
pub mod scoring;
pub mod urn;

pub use component::{
    clamp_first, SuggestedItem, SuggestionRequest, SuggestionsComponent, SuggestionsResponse,
};
pub use constants::{
    normalize_body_shape, normalize_category, MAX_EQUIPPED, MAX_EXCLUDE, MAX_SEEDS,
    SUGGESTIONS_MAX_CONCURRENT,
};
pub use scoring::{SuggestionReason, SuggestionReasonKind};
pub use urn::{normalize_item_ids, to_item_ids};
