use axum::extract::{Multipart, Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};

use crate::content_store::{is_valid_hash, MAX_POSTER_BYTES};
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

fn mime_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "gif" => Some("image/gif"),
        "png" => Some("image/png"),
        "jpg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

async fn serve_stored(
    store: &crate::content_store::ContentStore,
    filename: &str,
) -> Result<impl IntoResponse, ApiError> {
    let not_found = || ApiError::not_found("Not found");
    let (stem, ext) = filename.rsplit_once('.').ok_or_else(not_found)?;
    let mime = mime_for_extension(ext).ok_or_else(not_found)?;
    if !is_valid_hash(stem) {
        return Err(not_found());
    }
    let bytes = store
        .get(stem)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "poster content store read failed");
            ApiError::internal("Service unavailable")
        })?
        .ok_or_else(not_found)?;
    Ok((
        [
            (CONTENT_TYPE, mime),
            (CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    ))
}

#[utoipa::path(
    get,
    path = "/poster/{filename}",
    tag = "posters",
    responses(
        (status = 200, body = Vec<u8>, content_type = "image/png"),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_poster(
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    serve_stored(&state.content_store, &filename).await
}

#[utoipa::path(
    get,
    path = "/poster-vertical/{filename}",
    tag = "posters",
    responses(
        (status = 200, body = Vec<u8>, content_type = "image/png"),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_poster_vertical(
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    serve_stored(&state.content_store, &filename).await
}

fn detect_and_validate_mime(
    data: &[u8],
    declared: &str,
    allowed: &[&str],
) -> Result<&'static str, ApiError> {
    let normalized = declared
        .split(';')
        .next()
        .unwrap_or(declared)
        .trim()
        .to_ascii_lowercase();
    let reject = || {
        ApiError::bad_request(format!(
            "Invalid file content; expected one of {}",
            allowed.join(", ")
        ))
    };
    let detected = infer::get(data).map(|t| t.mime_type()).ok_or_else(reject)?;
    if detected != normalized.as_str() || !allowed.contains(&detected) {
        return Err(reject());
    }
    Ok(detected)
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
        return Err(ApiError::http(413, "File size limit has been reached"));
    }
    let ext = extension(&poster.mime);
    let hash = state.content_store.put(&poster.data).await.map_err(|e| {
        tracing::error!(error = %e, "poster content store failed");
        ApiError::internal("Service unavailable")
    })?;

    let filename = format!("{}/{}{}", dir, hash, ext);
    Ok(Json(json!({
        "filename": filename,
        "url": format!("/{}", filename),
        "size": size,
        "type": poster.mime,
    })))
}

#[utoipa::path(
    post,
    path = "/api/poster",
    tag = "posters",
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 413, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn upload_poster(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    crate::auth_chain::require_signer(&headers, "post", "/api/poster").await?;
    let mut poster = read_poster(multipart).await?;
    let mime = detect_and_validate_mime(&poster.data, &poster.mime, &POSTER_FILE_TYPES)?;
    poster.mime = mime.to_string();
    store_and_respond(&state, poster, "poster").await
}

#[utoipa::path(
    post,
    path = "/api/poster-vertical",
    tag = "posters",
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 413, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn upload_poster_vertical(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    crate::auth_chain::require_signer(&headers, "post", "/api/poster-vertical").await?;
    let mut poster = read_poster(multipart).await?;
    let mime = detect_and_validate_mime(&poster.data, &poster.mime, &POSTER_VERTICAL_FILE_TYPES)?;
    poster.mime = mime.to_string();
    store_and_respond(&state, poster, "poster-vertical").await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn scratch_store(tag: &str) -> (crate::content_store::ContentStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("poster-serve-{tag}-{}", std::process::id()));
        let store = crate::content_store::ContentStore::new(&dir, MAX_POSTER_BYTES);
        store.init().await.unwrap();
        (store, dir)
    }

    #[tokio::test]
    async fn serve_stored_round_trips_an_uploaded_poster() {
        let (store, dir) = scratch_store("roundtrip").await;
        let hash = store.put(&PNG_MAGIC).await.unwrap();
        let resp = serve_stored(&store, &format!("{hash}.png"))
            .await
            .unwrap()
            .into_response();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers()[CONTENT_TYPE], "image/png");
        assert_eq!(
            resp.headers()[CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn serve_stored_answers_404_for_unknown_or_foreign_filenames() {
        let (store, dir) = scratch_store("foreign").await;
        for name in [
            "85c9f954-9529-453a-9777-90778bf46f94.webp",
            "1ad0d5dd6a03ac29.jpg",
            "no-extension",
            "0000000000000000000000000000000000000000000000000000000000000000.pdf",
            "0000000000000000000000000000000000000000000000000000000000000000.png",
        ] {
            let err = serve_stored(&store, name)
                .await
                .err()
                .unwrap_or_else(|| panic!("{name} must not serve"));
            let resp = err.into_response();
            assert_eq!(resp.status(), 404, "{name}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

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

    const PNG_MAGIC: [u8; 16] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52,
    ];

    #[test]
    fn detect_and_validate_returns_detected_mime_when_png_declares_png() {
        assert_eq!(
            detect_and_validate_mime(&PNG_MAGIC, "image/png", &POSTER_FILE_TYPES).unwrap(),
            "image/png"
        );
    }

    #[test]
    fn detect_and_validate_normalizes_declared_case_and_params() {
        assert_eq!(
            detect_and_validate_mime(&PNG_MAGIC, "IMAGE/PNG; charset=binary", &POSTER_FILE_TYPES)
                .unwrap(),
            "image/png"
        );
    }

    #[test]
    fn detect_and_validate_rejects_arbitrary_bytes_declaring_allowed_type() {
        assert!(detect_and_validate_mime(
            b"<script>alert(1)</script>",
            "image/png",
            &POSTER_FILE_TYPES
        )
        .is_err());
    }

    #[test]
    fn detect_and_validate_rejects_declared_detected_mismatch() {
        assert!(detect_and_validate_mime(&PNG_MAGIC, "image/jpeg", &POSTER_FILE_TYPES).is_err());
    }

    #[test]
    fn detect_and_validate_enforces_endpoint_allowlist() {
        let gif = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00];
        assert_eq!(
            detect_and_validate_mime(&gif, "image/gif", &POSTER_FILE_TYPES).unwrap(),
            "image/gif"
        );
        assert!(detect_and_validate_mime(&gif, "image/gif", &POSTER_VERTICAL_FILE_TYPES).is_err());
    }
}
