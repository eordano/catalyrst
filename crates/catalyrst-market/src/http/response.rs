use serde::Serialize;

// Re-export the shared output envelope from catalyrst-types so existing
// imports (`use crate::http::response::PaginatedResponse`) keep working.
pub use catalyrst_types::PaginatedResponse;

// The error envelope (incl. `impl IntoResponse`) lives in
// `catalyrst-types::error` so any future HTTP-returning crate in the
// workspace can reuse it. Re-exported as `ApiError` here to keep
// `use crate::http::response::ApiError` working for existing ports and
// handlers (and to keep the `?`-conversion machinery — `From<sqlx::Error>`,
// `From<HttpError>`, `From<InvalidParameterError>` — exactly as before).
pub use catalyrst_types::MarketplaceApiError as ApiError;

#[derive(Debug, Serialize)]
pub struct DataTotal<T> {
    pub data: Vec<T>,
    pub total: i64,
}
