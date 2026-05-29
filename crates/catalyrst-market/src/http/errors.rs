// `HttpError` and `InvalidParameterError` live in `catalyrst-types::error`
// so any future HTTP-returning crate in the workspace can reuse them.
// Re-exported here to keep `use crate::http::errors::{HttpError, InvalidParameterError}`
// working for existing ports and handlers.
pub use catalyrst_types::{HttpError, InvalidParameterError};
