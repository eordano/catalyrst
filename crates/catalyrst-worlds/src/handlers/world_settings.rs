use std::collections::HashMap;

use axum::extract::{Multipart, OriginalUri, Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::auth_chain::{require_verified, AuthChainError};
use crate::http::ApiError;
use crate::ports::worlds::{WorldSettingsRow, WorldSettingsUpdate};
use crate::AppState;

const MIN_PARCEL_COORDINATE: i32 = -150;
const MAX_PARCEL_COORDINATE: i32 = 150;
const MAX_THUMBNAIL_BYTES: usize = 1024 * 1024;
const VALID_RATINGS: [&str; 5] = ["RP", "E", "T", "A", "R"];

pub async fn get_world_settings(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let settings = state
        .worlds
        .get_world_settings(&world_name)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("World \"{world_name}\" not found.")))?;
    Ok(Json(settings_json(&settings)))
}

fn settings_json(s: &WorldSettingsRow) -> Value {
    json!({
        "title": s.title,
        "description": s.description,
        "content_rating": s.content_rating,
        "spawn_coordinates": s.spawn_coordinates,
        "skybox_time": s.skybox_time,
        "categories": s.categories,
        "single_player": s.single_player,
        "show_in_places": s.show_in_places,
        "thumbnail_hash": s.thumbnail_hash,
    })
}

pub async fn update_world_settings(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let auth = require_verified(&headers, "put", uri.path()).map_err(map_auth_error)?;
    let signer = auth.signer.to_lowercase();

    let world = state.worlds.get_world(&world_name).await?;
    let owner = crate::handlers::permissions::resolve_world_owner(
        &state,
        &world_name,
        world.and_then(|w| w.owner),
    )
    .await;
    let is_owner = owner
        .as_deref()
        .map(|o| o.eq_ignore_ascii_case(&signer))
        .unwrap_or(false);
    let is_world_wide_deployer = if is_owner {
        false
    } else {
        state
            .worlds
            .has_world_wide_permission(&world_name, "deployment", &signer)
            .await?
    };
    if !is_owner && !is_world_wide_deployer {
        return Err(ApiError::forbidden(
            "You are not authorized to update the settings of this world.",
        ));
    }

    let input = parse_multipart(multipart, &state).await?;

    if let Some(spawn) = input.spawn_coordinates.clone() {
        let (x, y) = parse_coordinate(&spawn).ok_or_else(|| {
            ApiError::bad_request(format!("Invalid spawnCoordinates format: \"{spawn}\"."))
        })?;
        match state
            .worlds
            .get_world_bounding_rectangle(&world_name)
            .await?
        {
            None => {
                return Err(ApiError::bad_request(format!(
                    "Invalid spawnCoordinates \"{spawn}\". The world has no deployed scenes."
                )))
            }
            Some((min_x, max_x, min_y, max_y)) => {
                if !(min_x..=max_x).contains(&x) || !(min_y..=max_y).contains(&y) {
                    return Err(ApiError::bad_request(format!(
                        "Invalid spawnCoordinates \"{spawn}\". It must be within the world shape rectangle: ({min_x},{min_y}) to ({max_x},{max_y})."
                    )));
                }
            }
        }
    }

    let (settings, _old_spawn) = state
        .worlds
        .update_world_settings(&world_name, &signer, &input)
        .await?;

    Ok(Json(json!({
        "message": "World settings updated successfully",
        "settings": settings_json(&settings),
    })))
}

fn map_auth_error(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::MissingTimestamp
        | AuthChainError::MalformedChain { .. }
        | AuthChainError::InsufficientLinks => ApiError::bad_request(e.to_string()),
        _ => ApiError::unauthorized(e.to_string()),
    }
}

async fn parse_multipart(
    mut multipart: Multipart,
    state: &AppState,
) -> Result<WorldSettingsUpdate, ApiError> {
    let mut fields: HashMap<String, Vec<String>> = HashMap::new();
    let mut thumbnail: Option<Vec<u8>> = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return Err(ApiError::bad_request(format!(
                    "Invalid multipart form: {e}"
                )))
            }
        };
        let name = field.name().unwrap_or("").to_string();
        let is_file = field.file_name().is_some();

        if is_file {
            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad_request(format!("Failed to read file data: {e}")))?;
            if name == "thumbnail" {
                if data.len() > MAX_THUMBNAIL_BYTES {
                    return Err(ApiError::bad_request(format!(
                        "Invalid thumbnail: size {} bytes exceeds maximum of {MAX_THUMBNAIL_BYTES} bytes (1MB).",
                        data.len()
                    )));
                }
                thumbnail = Some(data.to_vec());
            }
        } else {
            let value = field
                .text()
                .await
                .map_err(|e| ApiError::bad_request(format!("Invalid form field: {e}")))?;
            fields.entry(name).or_default().push(value);
        }
    }

    let mut input = WorldSettingsUpdate::default();

    if let Some(title) = first_nonempty(&fields, "title") {
        if title.len() < 3 || title.len() > 100 {
            return Err(ApiError::bad_request(format!(
                "Invalid title: {title}. Expected between 3 and 100 characters."
            )));
        }
        input.title = Some(title);
    }

    if let Some(description) = first_nonempty(&fields, "description") {
        if description.len() < 3 || description.len() > 1000 {
            return Err(ApiError::bad_request(format!(
                "Invalid description: {description}. Expected between 3 and 1000 characters."
            )));
        }
        input.description = Some(description);
    }

    if let Some(rating) = first_value(&fields, "content_rating") {
        if !VALID_RATINGS.contains(&rating.as_str()) {
            return Err(ApiError::bad_request(format!(
                "Invalid content rating: {rating}. Expected one of: {}",
                VALID_RATINGS.join(", ")
            )));
        }
        input.content_rating = Some(rating);
    }

    if let Some(spawn) = first_value(&fields, "spawn_coordinates") {
        if parse_coordinate(&spawn).is_none() {
            return Err(ApiError::bad_request(format!(
                "Invalid spawnCoordinates format: \"{spawn}\"."
            )));
        }
        input.spawn_coordinates = Some(spawn);
    }

    if let Some(value) = first_value(&fields, "skybox_time") {
        input.skybox_time_provided = true;
        input.skybox_time = if value == "null" {
            None
        } else {
            value.parse::<i32>().ok()
        };
    }

    if let Some(values) = fields.get("categories") {
        input.categories_provided = true;
        if values.len() == 1 && values[0] == "null" {
            input.categories = Some(Vec::new());
        } else {
            if values.len() > 20 {
                return Err(ApiError::bad_request(format!(
                    "Invalid categories: {} items. Expected at most 20",
                    values.len()
                )));
            }
            input.categories = Some(values.clone());
        }
    }

    if let Some(value) = first_value(&fields, "single_player") {
        input.single_player = Some(value == "true");
    }
    if let Some(value) = first_value(&fields, "show_in_places") {
        input.show_in_places = Some(value == "true");
    }

    if let Some(bytes) = thumbnail {
        if bytes.len() > MAX_THUMBNAIL_BYTES {
            return Err(ApiError::bad_request(format!(
                "Invalid thumbnail: size {} bytes exceeds maximum of {MAX_THUMBNAIL_BYTES} bytes (1MB).",
                bytes.len()
            )));
        }
        if detect_image_format(&bytes).is_none() {
            return Err(ApiError::bad_request(
                "Invalid thumbnail: expected a PNG, JPEG, GIF or WebP image.",
            ));
        }
        let hash = hex::encode(Sha256::digest(&bytes));
        store_thumbnail(&state.cfg.contents_dir, &hash, &bytes)
            .await
            .map_err(|e| ApiError::internal(format!("failed to store thumbnail: {e}")))?;
        input.thumbnail_hash = Some(hash);
    }

    Ok(input)
}

fn first_value(fields: &HashMap<String, Vec<String>>, key: &str) -> Option<String> {
    fields.get(key).and_then(|v| v.first()).cloned()
}

fn first_nonempty(fields: &HashMap<String, Vec<String>>, key: &str) -> Option<String> {
    first_value(fields, key).filter(|s| !s.is_empty())
}

fn parse_coordinate(s: &str) -> Option<(i32, i32)> {
    let (a, b) = s.split_once(',')?;
    let parse = |p: &str| -> Option<i32> {
        let t = p.trim();
        let digits = t.strip_prefix('-').unwrap_or(t);
        if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        t.parse::<i32>().ok()
    };
    let x = parse(a)?;
    let y = parse(b)?;
    if !(MIN_PARCEL_COORDINATE..=MAX_PARCEL_COORDINATE).contains(&x)
        || !(MIN_PARCEL_COORDINATE..=MAX_PARCEL_COORDINATE).contains(&y)
    {
        return None;
    }
    Some((x, y))
}

fn detect_image_format(buf: &[u8]) -> Option<&'static str> {
    const PNG: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    if buf.len() >= 8 && buf[..8] == PNG {
        return Some("png");
    }
    if buf.len() >= 3 && buf[0] == 0xff && buf[1] == 0xd8 && buf[2] == 0xff {
        return Some("jpeg");
    }
    if buf.len() >= 6 && (&buf[..6] == b"GIF87a" || &buf[..6] == b"GIF89a") {
        return Some("gif");
    }
    if buf.len() >= 12 && &buf[..4] == b"RIFF" && &buf[8..12] == b"WEBP" {
        return Some("webp");
    }
    None
}

async fn store_thumbnail(dir: &std::path::Path, hash: &str, bytes: &[u8]) -> std::io::Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    let dst = dir.join(hash);
    if tokio::fs::try_exists(&dst).await.unwrap_or(false) {
        return Ok(());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".{hash}.{}.{nonce}.part", std::process::id()));
    tokio::fs::write(&tmp, bytes).await?;
    match tokio::fs::rename(&tmp, &dst).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_parse_and_bounds() {
        assert_eq!(parse_coordinate("0,0"), Some((0, 0)));
        assert_eq!(parse_coordinate(" -5 , 10 "), Some((-5, 10)));
        assert_eq!(parse_coordinate("150,-150"), Some((150, -150)));
        assert!(parse_coordinate("151,0").is_none());
        assert!(parse_coordinate("0,-151").is_none());
        assert!(parse_coordinate("abc").is_none());
        assert!(parse_coordinate("1,2,3").is_none());
    }

    #[test]
    fn image_magic_bytes_detection() {
        assert_eq!(
            detect_image_format(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0]),
            Some("png")
        );
        assert_eq!(detect_image_format(&[0xff, 0xd8, 0xff, 0, 0]), Some("jpeg"));
        assert_eq!(detect_image_format(b"GIF89a...."), Some("gif"));
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&[0, 0, 0, 0]);
        webp.extend_from_slice(b"WEBP");
        assert_eq!(detect_image_format(&webp), Some("webp"));
        assert!(detect_image_format(b"<svg xmlns=").is_none());
    }
}
