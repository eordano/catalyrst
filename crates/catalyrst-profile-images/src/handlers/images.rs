use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use crate::cache::{is_valid_entity_id, ImageKind};
use crate::origin::OriginResult;
use crate::queue::RenderOutcome;
use crate::AppState;

pub async fn face(State(state): State<AppState>, Path(entity): Path<String>) -> Response {
    serve(state, entity, ImageKind::Face).await
}

pub async fn body(State(state): State<AppState>, Path(entity): Path<String>) -> Response {
    serve(state, entity, ImageKind::Body).await
}

async fn serve(state: AppState, entity: String, kind: ImageKind) -> Response {
    if !is_valid_entity_id(&entity) {
        return (StatusCode::BAD_REQUEST, "invalid entity id").into_response();
    }

    // 1. Serve from disk cache if fresh.
    if let Some(bytes) = state.cache.get(&entity, kind).await {
        return png_response(bytes, "HIT");
    }

    // 2. Cache miss — primary path: render locally.
    if let Some(queue) = state.render_queue.as_ref() {
        match queue.render_once(&entity).await {
            RenderOutcome::Rendered => {
                // The render wrote both PNGs into the cache; re-read the one
                // this request wants.
                if let Some(bytes) = state.cache.get(&entity, kind).await {
                    return png_response(bytes, "RENDER");
                }
                tracing::error!(entity = %entity, kind = ?kind, "render reported success but cache miss");
                // Fall through to fallback / error below.
            }
            RenderOutcome::NotFound => {
                return (StatusCode::NOT_FOUND, "image not available").into_response();
            }
            RenderOutcome::Failed(e) => {
                tracing::error!(entity = %entity, kind = ?kind, error = %e, "local render failed");
                // Only fall back to the proxy if explicitly enabled.
                if !state.render_fallback_proxy {
                    return (StatusCode::BAD_GATEWAY, "avatar render failed").into_response();
                }
            }
        }
    }

    // 3. Proxy: the primary path for `proxy` backend, or the explicit
    //    last-resort fallback for `render` (only reached when a render failed
    //    and PROFILE_IMAGES_RENDER_FALLBACK_PROXY is set, or when render
    //    succeeded-but-vanished from cache).
    let Some(origin) = state.origin.as_ref() else {
        return (StatusCode::NOT_FOUND, "image not available").into_response();
    };

    let label = if state.render_queue.is_some() {
        "FALLBACK"
    } else {
        "MISS"
    };

    match origin.fetch(&entity, kind).await {
        OriginResult::Hit(bytes) => {
            if let Err(e) = state.cache.put(&entity, kind, &bytes).await {
                tracing::warn!(entity = %entity, kind = ?kind, error = %e, "cache write failed");
            }
            png_response(bytes, label)
        }
        OriginResult::NotFound => (StatusCode::NOT_FOUND, "image not available").into_response(),
        OriginResult::Error(e) => {
            tracing::error!(entity = %entity, kind = ?kind, error = %e, "origin fetch failed");
            (StatusCode::BAD_GATEWAY, "upstream profile-images error").into_response()
        }
    }
}

fn png_response(bytes: Bytes, cache_status: &'static str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/png")),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=86400"),
            ),
            (
                header::HeaderName::from_static("x-cache"),
                HeaderValue::from_static(cache_status),
            ),
        ],
        bytes,
    )
        .into_response()
}
