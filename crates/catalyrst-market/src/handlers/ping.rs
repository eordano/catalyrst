use axum::extract::OriginalUri;
use axum::response::IntoResponse;

pub async fn ping(OriginalUri(uri): OriginalUri) -> impl IntoResponse {
    uri.path().to_string()
}
