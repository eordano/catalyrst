//! Direct port of `marketplace-server/src/controllers/handlers/ping-handler.ts`.
//! The TS version increments a `test_ping_counter` metric per pathname; we
//! don't have a metrics layer yet, so we just return the path.

use axum::extract::OriginalUri;
use axum::response::IntoResponse;

pub async fn ping(OriginalUri(uri): OriginalUri) -> impl IntoResponse {
    uri.path().to_string()
}
