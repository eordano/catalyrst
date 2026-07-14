use axum::extract::{FromRequest, Multipart, Request};
use axum::response::Response;
use serde::de::DeserializeOwned;

use crate::errors::json_error;

pub struct JsonBody<T>(pub T);

impl<S, T> FromRequest<S> for JsonBody<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(v)) => Ok(JsonBody(v)),
            Err(rej) => Err(json_error(rej.status(), &rej.body_text())),
        }
    }
}

pub struct MultipartBody(pub Multipart);

impl<S> FromRequest<S> for MultipartBody
where
    S: Send + Sync,
{
    type Rejection = Response;
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Multipart::from_request(req, state).await {
            Ok(m) => Ok(MultipartBody(m)),
            Err(rej) => Err(json_error(rej.status(), &rej.body_text())),
        }
    }
}
