use serde::Serialize;

pub use catalyrst_types::PaginatedResponse;

pub use catalyrst_types::MarketplaceApiError as ApiError;

#[derive(Debug, Serialize)]
pub struct DataTotal<T> {
    pub data: Vec<T>,
    pub total: i64,
}
