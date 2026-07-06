use axum::extract::{Multipart, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::content_store::MAX_POSTER_BYTES;
use crate::http::response::ApiError;
use crate::AppState;

const POSTER_FILE_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/gif", "image/webp"];

const POSTER_VERTICAL_FILE_TYPES: [&str; 3] = ["image/jpeg", "image/png", "image/webp"];

fn extension(mime: &str) -> &'static str {
    match mime {
        "image/gif" => ".gif",
        "image/png" => ".png",
        "image/jpeg" => ".jpg",
        "image/webp" => ".webp",
        _ => "",
    }
}

struct UploadedPoster {
    data: Vec<u8>,
    mime: String,
}

async fn read_poster(mut multipart: Multipart) -> Result<UploadedPoster, ApiError> {
    let mut poster: Option<UploadedPoster> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("invalid multipart: {e}")))?
    {
        if field.name() == Some("poster") {
            if poster.is_some() {
                return Err(ApiError::bad_request("Multiple files are not allowed"));
            }
            let mime = field
                .content_type()
                .map(|s| s.split(';').next().unwrap_or(s).to_string())
                .unwrap_or_default();
            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad_request(format!("invalid poster field: {e}")))?;
            poster = Some(UploadedPoster {
                data: data.to_vec(),
                mime,
            });
        } else {
            let _ = field.bytes().await;
        }
    }
    let poster = poster.ok_or_else(|| ApiError::bad_request("Poster param is required"))?;
    if poster.data.is_empty() {
        return Err(ApiError::bad_request("Empty files are not allowed"));
    }
    Ok(poster)
}

async fn store_and_respond(
    state: &AppState,
    poster: UploadedPoster,
    dir: &str,
) -> Result<Json<Value>, ApiError> {
    let size = poster.data.len();
    if size > MAX_POSTER_BYTES {
        return Err(ApiError::Http(catalyrst_types::HttpError::new(
            413,
            "File size limit has been reached",
        )));
    }
    let ext = extension(&poster.mime);
    let hash = state.content_store.put(&poster.data).await.map_err(|e| {
        tracing::error!(error = %e, "poster content store failed");
        ApiError::Internal("Service unavailable".into())
    })?;

    let filename = format!("{}/{}{}", dir, hash, ext);
    Ok(Json(json!({
        "filename": filename,
        "url": format!("/{}", filename),
        "size": size,
        "type": poster.mime,
    })))
}

pub async fn upload_poster(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    crate::auth_chain::require_signer(&headers, "post", "/api/poster")
        .map_err(|_| ApiError::unauthorized("Unauthorized"))?;
    let poster = read_poster(multipart).await?;
    if !POSTER_FILE_TYPES.contains(&poster.mime.as_str()) {
        return Err(ApiError::bad_request(format!(
            "Invalid file type {}",
            poster.mime
        )));
    }
    store_and_respond(&state, poster, "poster").await
}

pub async fn upload_poster_vertical(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    crate::auth_chain::require_signer(&headers, "post", "/api/poster-vertical")
        .map_err(|_| ApiError::unauthorized("Unauthorized"))?;
    let poster = read_poster(multipart).await?;
    if !POSTER_VERTICAL_FILE_TYPES.contains(&poster.mime.as_str()) {
        return Err(ApiError::bad_request(format!(
            "Invalid file type {}. Only PNG, JPG and WebP are allowed for vertical posters",
            poster.mime
        )));
    }
    store_and_respond(&state, poster, "poster-vertical").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_maps_each_allowed_mime() {
        assert_eq!(extension("image/gif"), ".gif");
        assert_eq!(extension("image/png"), ".png");
        assert_eq!(extension("image/jpeg"), ".jpg");
        assert_eq!(extension("image/webp"), ".webp");
        assert_eq!(extension("application/pdf"), "");
    }

    #[test]
    fn vertical_rejects_gif_but_horizontal_allows_it() {
        assert!(POSTER_FILE_TYPES.contains(&"image/gif"));
        assert!(!POSTER_VERTICAL_FILE_TYPES.contains(&"image/gif"));

        for t in ["image/jpeg", "image/png", "image/webp"] {
            assert!(POSTER_FILE_TYPES.contains(&t));
            assert!(POSTER_VERTICAL_FILE_TYPES.contains(&t));
        }
    }
}
