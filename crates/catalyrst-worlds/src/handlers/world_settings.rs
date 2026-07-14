use std::collections::HashMap;
use std::time::Duration;

use axum::extract::{Multipart, OriginalUri, Path, State};
use axum::http::HeaderMap;
use axum::Json;
use bytes::BytesMut;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::auth_chain::{require_verified, AuthChainError};
use crate::http::ApiError;
use crate::ports::worlds::{WorldSettingsRow, WorldSettingsUpdate};
use crate::settings_policy::{
    detect_image_format, storable_skybox_time, text_len, DESCRIPTION_MAX_LENGTH,
    DESCRIPTION_MIN_LENGTH, MAX_CATEGORIES, TITLE_MAX_LENGTH, TITLE_MIN_LENGTH, VALID_RATINGS,
};
use crate::upload_limits;
use crate::AppState;

const MIN_PARCEL_COORDINATE: i32 = -150;
const MAX_PARCEL_COORDINATE: i32 = 150;
const MAX_THUMBNAIL_BYTES: usize = 1024 * 1024;

const REALM_NAME_MAX_LENGTH: usize = 64;

const MAX_PREVIEW_WEARABLES: usize = 8;

/// The realm name reaches the client as a bare token it may interpolate into a
/// URL path, so anything that could change that URL's shape is refused here
/// rather than at whatever consumes it later.
fn is_valid_realm_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= REALM_NAME_MAX_LENGTH
        && name
            .chars()
            .all(|c| c.is_ascii_graphic() && c != '/' && c != '\\' && c != '?' && c != '#')
}

const MAX_SETTINGS_UPLOAD_BYTES: usize = 2 * 1024 * 1024;

/// Wire-size cap for the whole multipart body; also this route's axum body limit so our 400 fires instead of axum's 413.
pub const MAX_SETTINGS_UPLOAD_WIRE_BYTES: usize = MAX_SETTINGS_UPLOAD_BYTES + 10 * 1024 * 1024;

const _: () = assert!(MAX_SETTINGS_UPLOAD_WIRE_BYTES >= MAX_SETTINGS_UPLOAD_BYTES);

#[utoipa::path(
    get,
    path = "/world/{world_name}/settings",
    tag = "worlds",
    params(("world_name" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
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
        // NULL means neither the owner nor any scene expressed a preference, so
        // report the effective default; the distinction only matters in storage,
        // where NULL lets a scene that omits these preserve the owner's choice.
        "single_player": s.single_player.unwrap_or(false),
        "show_in_places": s.show_in_places.unwrap_or(true),
        "thumbnail_hash": s.thumbnail_hash,
        "access_type": s.access_type,
        "realm_name_override": s.realm_name_override,
        "preview_wearable_urns": s.preview_wearable_urns,
        "settings_version": s.settings_version,
    })
}

#[utoipa::path(
    put,
    path = "/world/{world_name}/settings",
    tag = "worlds",
    params(("world_name" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 408, body = serde_json::Value),
        (status = 500, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = serde_json::Value)
    )
)]
pub async fn update_world_settings(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let auth = require_verified(&headers, "put", uri.path())
        .await
        .map_err(map_auth_error)?;
    let signer = auth.signer.as_str().to_string();

    match upload_limits::declared_content_length(&headers) {
        upload_limits::DeclaredContentLength::Invalid => {
            return Err(ApiError::bad_request(
                upload_limits::INVALID_CONTENT_LENGTH_MESSAGE,
            ));
        }
        upload_limits::DeclaredContentLength::Bytes(len)
            if len > MAX_SETTINGS_UPLOAD_WIRE_BYTES as u64 =>
        {
            return Err(ApiError::bad_request(
                upload_limits::PAYLOAD_TOO_LARGE_MESSAGE,
            ));
        }
        _ => {}
    }
    let _slot = upload_limits::try_acquire_upload_slot(state.cfg.max_concurrent_uploads)
        .ok_or_else(|| {
            tracing::warn!(
                active = upload_limits::active_uploads(),
                max = state.cfg.max_concurrent_uploads,
                "PUT /world/:world_name/settings shed: concurrent-upload cap exceeded"
            );
            ApiError::UploadShed(upload_limits::CONCURRENCY_SHED_MESSAGE.to_string())
        })?;
    let mut bytes_lease = upload_limits::reserve_in_flight();
    let mut files_lease = upload_limits::reserve_in_flight_files();

    let input = tokio::time::timeout(
        Duration::from_millis(state.cfg.multipart_upload_timeout_ms),
        parse_multipart(
            multipart,
            &state.cfg.contents_dir,
            state.cfg.max_in_flight_upload_bytes,
            state.cfg.max_in_flight_upload_files,
            &mut bytes_lease,
            &mut files_lease,
        ),
    )
    .await
    .map_err(|_| {
        tracing::warn!(
            timeout_ms = state.cfg.multipart_upload_timeout_ms,
            "PUT /world/:world_name/settings: multipart upload timed out"
        );
        ApiError::RequestTimeout(upload_limits::MULTIPART_TIMEOUT_MESSAGE.to_string())
    })??;

    let timeout_ms = state.cfg.deployment_processing_timeout_ms;
    tokio::time::timeout(Duration::from_millis(timeout_ms), async {
        let world = state.worlds.get_world(&world_name).await?;
        let owner = crate::handlers::permissions::resolve_world_owner(
            &state,
            &crate::fed::names::LocalWorldName::from_request_path(&world_name),
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

        let (settings, _old_spawn) = state
            .worlds
            .update_world_settings(&world_name, &signer, &input)
            .await?;

        Ok(Json(json!({
            "message": "World settings updated successfully",
            "settings": settings_json(&settings),
        })))
    })
    .await
    .map_err(|_| {
        tracing::warn!(
            timeout_ms,
            "PUT /world/:world_name/settings: settings processing timed out"
        );
        ApiError::RequestTimeout(format!(
            "Deployment processing exceeded the {timeout_ms}ms deadline."
        ))
    })?
}

fn map_auth_error(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::MissingTimestamp
        | AuthChainError::MalformedChain { .. }
        | AuthChainError::InsufficientLinks => ApiError::bad_request(e.to_string()),
        _ => ApiError::unauthorized(e.to_string()),
    }
}

fn account_settings_bytes(
    total_bytes: &mut usize,
    added: usize,
    bytes_lease: &mut upload_limits::InFlightBytesGuard,
    max_in_flight_bytes: u64,
) -> Result<(), ApiError> {
    match upload_limits::account_payload_bytes(
        total_bytes,
        added,
        MAX_SETTINGS_UPLOAD_BYTES,
        bytes_lease,
        max_in_flight_bytes,
    ) {
        Ok(()) => Ok(()),
        Err(upload_limits::PayloadAccountError::PayloadTooLarge) => Err(ApiError::bad_request(
            upload_limits::PAYLOAD_TOO_LARGE_MESSAGE,
        )),
        Err(upload_limits::PayloadAccountError::BudgetExhausted) => {
            tracing::warn!(
                total_bytes,
                in_flight = upload_limits::in_flight_upload_bytes(),
                max = max_in_flight_bytes,
                "PUT /world/:world_name/settings shed: aggregate in-flight upload budget exceeded"
            );
            Err(ApiError::UploadShed(
                upload_limits::BYTES_SHED_MESSAGE.to_string(),
            ))
        }
    }
}

async fn parse_multipart(
    mut multipart: Multipart,
    contents_dir: &std::path::Path,
    max_in_flight_bytes: u64,
    max_in_flight_files: u64,
    bytes_lease: &mut upload_limits::InFlightBytesGuard,
    files_lease: &mut upload_limits::InFlightFilesGuard,
) -> Result<WorldSettingsUpdate, ApiError> {
    let mut fields: HashMap<String, Vec<String>> = HashMap::new();
    let mut thumbnail: Option<Vec<u8>> = None;
    let mut total_bytes: usize = 0;
    let mut total_files: u64 = 0;
    let mut part_count: usize = 0;
    let mut field_count: usize = 0;

    loop {
        let mut field = match multipart.next_field().await {
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

        part_count += 1;
        if part_count > upload_limits::MAX_MULTIPART_PARTS {
            return Err(ApiError::bad_request(upload_limits::TOO_MANY_PARTS_MESSAGE));
        }

        if is_file {
            if !files_lease.try_resize(total_files + 1, max_in_flight_files) {
                tracing::warn!(
                    request_files = total_files + 1,
                    in_flight = upload_limits::in_flight_upload_files(),
                    max = max_in_flight_files,
                    "PUT /world/:world_name/settings shed: aggregate in-flight upload-file budget exceeded"
                );
                return Err(ApiError::UploadShed(
                    upload_limits::FILES_SHED_MESSAGE.to_string(),
                ));
            }
            total_files += 1;
            let mut buf = BytesMut::new();
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        if buf.len().saturating_add(chunk.len()) > MAX_SETTINGS_UPLOAD_BYTES {
                            return Err(ApiError::bad_request(
                                "An uploaded file exceeds the maximum allowed size.",
                            ));
                        }
                        account_settings_bytes(
                            &mut total_bytes,
                            chunk.len(),
                            bytes_lease,
                            max_in_flight_bytes,
                        )?;
                        buf.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return Err(ApiError::bad_request(format!(
                            "Failed to read file data: {e}"
                        )))
                    }
                }
            }
            let data = buf.freeze();
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
            field_count += 1;
            if field_count > upload_limits::MAX_MULTIPART_FIELDS {
                return Err(ApiError::bad_request(
                    upload_limits::TOO_MANY_FIELDS_MESSAGE,
                ));
            }
            let mut buf = BytesMut::new();
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        if buf.len().saturating_add(chunk.len())
                            > upload_limits::MAX_MULTIPART_FIELD_VALUE_BYTES
                        {
                            return Err(ApiError::bad_request(
                                upload_limits::PAYLOAD_TOO_LARGE_MESSAGE,
                            ));
                        }
                        account_settings_bytes(
                            &mut total_bytes,
                            chunk.len(),
                            bytes_lease,
                            max_in_flight_bytes,
                        )?;
                        buf.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return Err(ApiError::bad_request(format!("Invalid form field: {e}")))
                    }
                }
            }
            let value = String::from_utf8_lossy(&buf).into_owned();
            fields.entry(name).or_default().push(value);
        }
    }

    let mut input = WorldSettingsUpdate::default();

    if let Some(title) = first_nonempty(&fields, "title") {
        if !(TITLE_MIN_LENGTH..=TITLE_MAX_LENGTH).contains(&text_len(&title)) {
            return Err(ApiError::bad_request(format!(
                "Invalid title: {title}. Expected between {TITLE_MIN_LENGTH} and {TITLE_MAX_LENGTH} characters."
            )));
        }
        input.title = Some(title);
    }

    if let Some(description) = first_nonempty(&fields, "description") {
        if !(DESCRIPTION_MIN_LENGTH..=DESCRIPTION_MAX_LENGTH).contains(&text_len(&description)) {
            return Err(ApiError::bad_request(format!(
                "Invalid description: {description}. Expected between {DESCRIPTION_MIN_LENGTH} and {DESCRIPTION_MAX_LENGTH} characters."
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

    if let Some(value) = first_nonempty(&fields, "skybox_time") {
        input.skybox_time_provided = true;
        input.skybox_time = if value == "null" {
            None
        } else {
            // Number semantics, not parse::<i32>: trailing garbage ("12abc"),
            // fractions ("1.5") and out-of-int4 values are refused with a 400
            // instead of silently clearing the stored value.
            let time = value
                .trim()
                .parse::<f64>()
                .ok()
                .and_then(storable_skybox_time)
                .ok_or_else(|| {
                    ApiError::bad_request(format!(
                        "Invalid skybox_time: {value}. Expected an integer between {} and {}, or null.",
                        i32::MIN,
                        i32::MAX
                    ))
                })?;
            Some(time)
        };
    }

    if let Some(value) = first_nonempty(&fields, "realm_name_override") {
        input.realm_name_override_provided = true;
        input.realm_name_override = if value == "null" {
            None
        } else {
            let name = value.trim().to_string();
            if !is_valid_realm_name(&name) {
                return Err(ApiError::bad_request(format!(
                    "Invalid realm_name_override: \"{name}\". Expected 1-{REALM_NAME_MAX_LENGTH} printable characters with no whitespace or \"/\", or null."
                )));
            }
            Some(name)
        };
    }

    if let Some(values) = fields.get("categories") {
        input.categories_provided = true;
        if values.len() == 1 && values[0] == "null" {
            input.categories = Some(Vec::new());
        } else {
            if values.len() > MAX_CATEGORIES {
                return Err(ApiError::bad_request(format!(
                    "Invalid categories: {} items. Expected at most {MAX_CATEGORIES}",
                    values.len()
                )));
            }
            input.categories = Some(values.clone());
        }
    }

    if let Some(values) = fields.get("preview_wearable_urns") {
        input.preview_wearable_urns_provided = true;
        input.preview_wearable_urns = if values.len() == 1 && values[0] == "null" {
            Some(Vec::new())
        } else {
            if values.len() > MAX_PREVIEW_WEARABLES {
                return Err(ApiError::bad_request(format!(
                    "Invalid preview_wearable_urns: {} items. Expected at most {MAX_PREVIEW_WEARABLES}",
                    values.len()
                )));
            }
            // The client only ever runs data[0], so a long list is a
            // misunderstanding rather than a richer configuration.
            Some(values.iter().map(|v| v.trim().to_string()).collect())
        };
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
        store_thumbnail(contents_dir, &hash, &bytes)
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
    let (x, y) = catalyrst_types::pointer::parse_pointer(s)?;
    let x = i32::try_from(x).ok()?;
    let y = i32::try_from(y).ok()?;
    if !(MIN_PARCEL_COORDINATE..=MAX_PARCEL_COORDINATE).contains(&x)
        || !(MIN_PARCEL_COORDINATE..=MAX_PARCEL_COORDINATE).contains(&y)
    {
        return None;
    }
    Some((x, y))
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
    fn settings_json_reports_effective_defaults_for_unexpressed_booleans() {
        let v = settings_json(&WorldSettingsRow::default());
        assert_eq!(v["single_player"], serde_json::json!(false));
        assert_eq!(v["show_in_places"], serde_json::json!(true));
        assert_eq!(v["settings_version"], serde_json::json!(0));
        assert!(v["access_type"].is_null());

        let v = settings_json(&WorldSettingsRow {
            single_player: Some(true),
            show_in_places: Some(false),
            settings_version: 3,
            access_type: Some("unrestricted".into()),
            ..Default::default()
        });
        assert_eq!(v["single_player"], serde_json::json!(true));
        assert_eq!(v["show_in_places"], serde_json::json!(false));
        assert_eq!(v["settings_version"], serde_json::json!(3));
        assert_eq!(v["access_type"], serde_json::json!("unrestricted"));
    }

    async fn multipart_from(parts: &[(&str, Option<&str>, &str)]) -> Multipart {
        use axum::extract::FromRequest;
        let boundary = "xyzsettings";
        let mut body = String::new();
        for (name, filename, value) in parts {
            body.push_str(&format!("--{boundary}\r\n"));
            match filename {
                Some(f) => body.push_str(&format!(
                    "Content-Disposition: form-data; name=\"{name}\"; filename=\"{f}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                )),
                None => {
                    body.push_str(&format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n"))
                }
            }
            body.push_str(value);
            body.push_str("\r\n");
        }
        body.push_str(&format!("--{boundary}--\r\n"));
        let req = axum::http::Request::builder()
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(axum::body::Body::from(body))
            .unwrap();
        Multipart::from_request(req, &()).await.unwrap()
    }

    fn bad_request_message(err: ApiError) -> String {
        match err {
            ApiError::BadRequest(m) => m,
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    fn unused_dir() -> &'static std::path::Path {
        std::path::Path::new("/nonexistent-unused")
    }

    #[tokio::test]
    async fn parse_multipart_streams_fields_and_accounts_payload() {
        let multipart = multipart_from(&[
            ("title", None, "Gate World"),
            ("other", Some("a.bin"), "abc"),
        ])
        .await;
        let mut bytes_lease = upload_limits::reserve_in_flight();
        let mut files_lease = upload_limits::reserve_in_flight_files();
        let input = parse_multipart(
            multipart,
            unused_dir(),
            u64::MAX,
            u64::MAX,
            &mut bytes_lease,
            &mut files_lease,
        )
        .await
        .expect("form parses");
        assert_eq!(input.title.as_deref(), Some("Gate World"));
        assert_eq!(bytes_lease.reserved(), 10 + 3);
        assert_eq!(files_lease.reserved(), 1);
    }

    async fn parse_settings_form(
        parts: &[(&str, Option<&str>, &str)],
    ) -> Result<WorldSettingsUpdate, ApiError> {
        let multipart = multipart_from(parts).await;
        let mut bytes_lease = upload_limits::reserve_in_flight();
        let mut files_lease = upload_limits::reserve_in_flight_files();
        parse_multipart(
            multipart,
            unused_dir(),
            u64::MAX,
            u64::MAX,
            &mut bytes_lease,
            &mut files_lease,
        )
        .await
    }

    #[tokio::test]
    async fn parse_multipart_rejects_malformed_skybox_time() {
        for value in ["1.5", "12abc", "3000000000", "-2147483649", "NaN"] {
            let err = parse_settings_form(&[("skybox_time", None, value)])
                .await
                .expect_err("out-of-policy skybox_time must be a 400");
            assert_eq!(
                bad_request_message(err),
                format!(
                    "Invalid skybox_time: {value}. Expected an integer between {} and {}, or null.",
                    i32::MIN,
                    i32::MAX
                )
            );
        }
    }

    #[tokio::test]
    async fn parse_multipart_accepts_integer_and_null_skybox_time() {
        let input = parse_settings_form(&[("skybox_time", None, "36000")])
            .await
            .expect("integer parses");
        assert!(input.skybox_time_provided);
        assert_eq!(input.skybox_time, Some(36000));

        let input = parse_settings_form(&[("skybox_time", None, "null")])
            .await
            .expect("explicit null clears");
        assert!(input.skybox_time_provided);
        assert_eq!(input.skybox_time, None);

        let input = parse_settings_form(&[("skybox_time", None, "")])
            .await
            .expect("empty field is not an update");
        assert!(!input.skybox_time_provided);
    }

    #[tokio::test]
    async fn parse_multipart_measures_text_in_utf16_code_units() {
        let err = parse_settings_form(&[("title", None, "\u{65E5}\u{672C}")])
            .await
            .expect_err("2 UTF-16 units is under the 3-unit title minimum");
        assert!(bad_request_message(err).starts_with("Invalid title"));

        let description = "\u{30C7}".repeat(400);
        let input = parse_settings_form(&[("description", None, description.as_str())])
            .await
            .expect("400 UTF-16 units fits the 1000-unit description bound despite 1200 bytes");
        assert_eq!(input.description.as_deref(), Some(description.as_str()));
    }

    #[tokio::test]
    async fn parse_multipart_caps_each_field_value_at_one_megabyte() {
        let big = "a".repeat(upload_limits::MAX_MULTIPART_FIELD_VALUE_BYTES + 1);
        let multipart = multipart_from(&[("description", None, big.as_str())]).await;
        let mut bytes_lease = upload_limits::reserve_in_flight();
        let mut files_lease = upload_limits::reserve_in_flight_files();
        let err = parse_multipart(
            multipart,
            unused_dir(),
            u64::MAX,
            u64::MAX,
            &mut bytes_lease,
            &mut files_lease,
        )
        .await
        .expect_err("oversized field must be rejected");
        assert_eq!(
            bad_request_message(err),
            upload_limits::PAYLOAD_TOO_LARGE_MESSAGE
        );
        assert!(bytes_lease.reserved() <= upload_limits::MAX_MULTIPART_FIELD_VALUE_BYTES as u64);
    }

    #[tokio::test]
    async fn parse_multipart_rejects_more_than_one_hundred_fields() {
        let names: Vec<String> = (0..=upload_limits::MAX_MULTIPART_FIELDS)
            .map(|i| format!("f{i}"))
            .collect();
        let parts: Vec<(&str, Option<&str>, &str)> =
            names.iter().map(|n| (n.as_str(), None, "v")).collect();
        let multipart = multipart_from(&parts).await;
        let mut bytes_lease = upload_limits::reserve_in_flight();
        let mut files_lease = upload_limits::reserve_in_flight_files();
        let err = parse_multipart(
            multipart,
            unused_dir(),
            u64::MAX,
            u64::MAX,
            &mut bytes_lease,
            &mut files_lease,
        )
        .await
        .expect_err("101st field must be rejected");
        assert_eq!(
            bad_request_message(err),
            upload_limits::TOO_MANY_FIELDS_MESSAGE
        );
    }

    #[tokio::test]
    async fn parse_multipart_rejects_more_than_the_parts_limit() {
        let parts: Vec<(&str, Option<&str>, &str)> =
            vec![("f", Some("a.bin"), ""); upload_limits::MAX_MULTIPART_PARTS + 1];
        let multipart = multipart_from(&parts).await;
        let mut bytes_lease = upload_limits::reserve_in_flight();
        let mut files_lease = upload_limits::reserve_in_flight_files();
        let err = parse_multipart(
            multipart,
            unused_dir(),
            u64::MAX,
            u64::MAX,
            &mut bytes_lease,
            &mut files_lease,
        )
        .await
        .expect_err("part 10101 must be rejected");
        assert_eq!(
            bad_request_message(err),
            upload_limits::TOO_MANY_PARTS_MESSAGE
        );
    }
}
