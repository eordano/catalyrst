use axum::Json;
use serde_json::Value;

use crate::http::response::ApiError;

const DEPRECATED: &str = "Web-push subscriptions are deprecated and no longer supported";

#[utoipa::path(
    get,
    path = "/api/profiles/subscriptions",
    tag = "profiles",
    responses((status = 410, body = catalyrst_types::ApiErrorBody))
)]
pub async fn get_profile_subscription() -> Result<Json<Value>, ApiError> {
    Err(ApiError::gone(DEPRECATED))
}

#[utoipa::path(
    post,
    path = "/api/profiles/subscriptions",
    tag = "profiles",
    responses((status = 410, body = catalyrst_types::ApiErrorBody))
)]
pub async fn create_profile_subscription() -> Result<Json<Value>, ApiError> {
    Err(ApiError::gone(DEPRECATED))
}

#[utoipa::path(
    delete,
    path = "/api/profiles/subscriptions",
    tag = "profiles",
    responses((status = 410, body = catalyrst_types::ApiErrorBody))
)]
pub async fn delete_profile_subscription() -> Result<Json<Value>, ApiError> {
    Err(ApiError::gone(DEPRECATED))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(e: ApiError) -> u16 {
        match e {
            ApiError::Common(catalyrst_types::ApiError::Http { status, .. }) => status,
            _ => 0,
        }
    }

    #[tokio::test]
    async fn all_verbs_return_410_gone() {
        assert_eq!(code(get_profile_subscription().await.unwrap_err()), 410);
        assert_eq!(code(create_profile_subscription().await.unwrap_err()), 410);
        assert_eq!(code(delete_profile_subscription().await.unwrap_err()), 410);
    }
}
