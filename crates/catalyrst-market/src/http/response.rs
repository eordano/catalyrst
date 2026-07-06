use serde::Serialize;

pub use catalyrst_types::PaginatedResponse;

pub use catalyrst_types::MarketplaceApiError as ApiError;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct DataTotal<T> {
    pub data: Vec<T>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
}
