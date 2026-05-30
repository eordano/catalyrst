use axum::body::Bytes;
use axum::extract::FromRequest;
use axum::http::header::CONTENT_TYPE;
use axum::http::Request;

use crate::http::ApiError;

pub struct JsonBody<T>(pub T);

const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

impl<T, S> FromRequest<S> for JsonBody<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request<axum::body::Body>, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_ascii_lowercase());
        let is_json = match content_type {
            Some(ct) => ct.starts_with("application/json") || ct.starts_with("application/+json"),
            None => false,
        };
        if !is_json {
            return Err(ApiError::bad_request(
                "Invalid request: expected application/json body",
            ));
        }

        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|_| ApiError::bad_request("Invalid request: could not read request body"))?;

        if bytes.len() > MAX_BODY_BYTES {
            return Err(ApiError::bad_request("Invalid request: request body too large"));
        }

        let value = serde_json::from_slice::<T>(&bytes)
            .map_err(|e| ApiError::bad_request(format!("Invalid request: {e}")))?;
        Ok(JsonBody(value))
    }
}
