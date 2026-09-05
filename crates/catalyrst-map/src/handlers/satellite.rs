use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::AppState;

pub async fn satellite_tile(
    State(state): State<AppState>,
    Path((z, x, y_ext)): Path<(i32, i32, String)>,
) -> Response {
    let y_str = y_ext.strip_suffix(".png").unwrap_or(&y_ext);
    let Ok(y) = y_str.parse::<i32>() else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let sat = state.satellite.clone();
    let bytes = match tokio::task::spawn_blocking(move || sat.tile_png(z, x, y)).await {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut resp = Response::new(Body::from((*bytes).clone()));
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));

    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300, stale-while-revalidate=600"),
    );
    resp
}
