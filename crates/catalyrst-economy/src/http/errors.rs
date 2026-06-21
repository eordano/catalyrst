use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

pub mod code {
    pub const UNKNOWN: &str = "unknown";
    pub const INVALID_TRANSACTION: &str = "invalid_transaction";
    pub const INVALID_SCHEMA: &str = "invalid_schema";
    pub const INVALID_CONTRACT_ADDRESS: &str = "invalid_contract_address";
    pub const SALE_PRICE_TOO_LOW: &str = "sale_price_too_low";
    pub const QUOTA_REACHED: &str = "quota_reached";
    pub const HIGH_CONGESTION: &str = "high_congestion";
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    InvalidSchema(String),
    #[error("{0}")]
    InvalidSalePrice(String),
    #[error("{0}")]
    InvalidContractAddress(String),
    #[error("{0}")]
    InvalidTransaction(String),
    #[error("{0}")]
    QuotaReached(String),
    #[error("{0}")]
    HighCongestion(String),
    #[error("{0}")]
    RelayReverted(String),
    #[error("{0}")]
    RelayerFailed(String),
    #[error("{0}")]
    RelayerUnavailable(String),
    #[error("{0}")]
    RelayerTimeout(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    MissingTransactionData(String),
    #[error("{0}")]
    MalformedBody(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    fn parts(&self) -> (u16, Option<&'static str>) {
        match self {
            ApiError::InvalidSchema(_) => (400, Some(code::INVALID_SCHEMA)),
            ApiError::InvalidSalePrice(_) => (400, Some(code::SALE_PRICE_TOO_LOW)),
            ApiError::InvalidContractAddress(_) => (400, Some(code::INVALID_CONTRACT_ADDRESS)),
            ApiError::InvalidTransaction(_) => (400, Some(code::INVALID_TRANSACTION)),
            ApiError::RelayReverted(_) => (400, Some(code::INVALID_TRANSACTION)),
            ApiError::QuotaReached(_) => (429, Some(code::QUOTA_REACHED)),
            ApiError::HighCongestion(_) => (503, Some(code::HIGH_CONGESTION)),
            ApiError::RelayerUnavailable(_) => (503, Some(code::UNKNOWN)),
            ApiError::RelayerTimeout(_) => (504, Some(code::UNKNOWN)),
            ApiError::Forbidden(_) => (403, Some(code::UNKNOWN)),
            ApiError::NotFound(_) => (404, Some(code::UNKNOWN)),
            ApiError::MissingTransactionData(_) => (400, None),
            ApiError::MalformedBody(_) => (500, None),
            ApiError::Database(_) => (500, Some(code::UNKNOWN)),
            ApiError::RelayerFailed(_) => (500, Some(code::UNKNOWN)),
            ApiError::Internal(_) => (500, Some(code::UNKNOWN)),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code_num, err_code) = self.parts();
        let message = match &self {
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                "Unknown error".to_string()
            }
            other => other.to_string(),
        };
        let status = StatusCode::from_u16(code_num).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = match err_code {
            Some(c) => json!({ "ok": false, "message": message, "code": c }),
            None => json!({ "ok": false, "message": message }),
        };
        (status, Json(body)).into_response()
    }
}
