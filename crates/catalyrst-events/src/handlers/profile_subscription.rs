use axum::Json;
use serde_json::Value;

use crate::http::response::ApiError;

/// Web-push subscriptions are `@deprecated` upstream (replaced by the centralised
/// notification service) and are not federated. All three verbs return 410 Gone.
const DEPRECATED: &str =
    "Web-push subscriptions are deprecated and no longer supported";

pub async fn get_profile_subscription() -> Result<Json<Value>, ApiError> {
    Err(ApiError::gone(DEPRECATED))
}

pub async fn create_profile_subscription() -> Result<Json<Value>, ApiError> {
    Err(ApiError::gone(DEPRECATED))
}

pub async fn delete_profile_subscription() -> Result<Json<Value>, ApiError> {
    Err(ApiError::gone(DEPRECATED))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(e: ApiError) -> u16 {
        match e {
            ApiError::Http(h) => h.code,
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
