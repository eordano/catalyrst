//! Foundation community pictures and associated-place metadata.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};

pub fn validate_thumbnail(encoded: &str) -> ApiResult<Vec<u8>> {
    // Foundation's upload contract is 1 KiB to 500 KiB. Bound before decoding too.
    if encoded.len() > 684_000 {
        return Err(bad("Choose a picture smaller than 500 KB"));
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| bad("Invalid picture encoding"))?;
    if !(1024..=500 * 1024).contains(&bytes.len()) || images::image_type(&bytes).is_none() {
        return Err(bad(
            "Choose a PNG, JPEG, GIF or WebP picture between 1 KB and 500 KB",
        ));
    }
    Ok(bytes)
}

/// The signed operation binds the base64 bytes; forward the same immutable bytes
/// as Foundation's normal multipart thumbnail field, rather than base64 text.
pub fn multipart(body: &str, id: &str, thumbnail: Option<&str>) -> ApiResult<Vec<u8>> {
    let Some(thumbnail) = thumbnail else {
        return Ok(body.as_bytes().to_vec());
    };
    let bytes = validate_thumbnail(thumbnail)?;
    let kind = images::image_type(&bytes).ok_or_else(|| bad("Invalid picture"))?;
    let boundary = format!("dcl-social-{id}");
    let closing = format!("--{boundary}--\r\n");
    let prefix = body
        .strip_suffix(&closing)
        .ok_or_else(|| bad("Invalid multipart body"))?;
    let mut result = prefix.as_bytes().to_vec();
    result.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"thumbnail\"; filename=\"community-picture\"\r\nContent-Type: {kind}\r\n\r\n").as_bytes());
    result.extend_from_slice(&bytes);
    result.extend_from_slice(format!("\r\n{closing}").as_bytes());
    Ok(result)
}

pub fn valid_place_id(id: &str) -> bool {
    Uuid::parse_str(id).is_ok() || discovery::valid_world(id)
}

pub async fn place(State(s): State<Arc<Store>>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    if !valid_place_id(&id) {
        return Err(bad("Invalid place ID"));
    }
    let world = Uuid::parse_str(&id).is_err();
    let value = memoized(
        &s,
        format!("place:{}", id.to_lowercase()),
        30_000,
        || async {
            let request = if world {
                s.client
                    .get("https://places.decentraland.org/api/worlds")
                    .query(&[("names", id.to_lowercase()), ("limit", "1".into())])
            } else {
                s.client
                    .get(format!("https://places.decentraland.org/api/places/{id}"))
            };
            let response = request.send().await.map_err(|_| unavailable())?;
            if !response.status().is_success() {
                return Err(if response.status() == StatusCode::NOT_FOUND {
                    ApiError(StatusCode::NOT_FOUND, "Place is no longer available".into())
                } else {
                    unavailable()
                });
            }
            let value = bounded_json(response).await?;
            if !world {
                return Ok(value);
            }
            let place = value
                .get("data")
                .and_then(Value::as_array)
                .and_then(|rows| {
                    rows.iter().find(|row| {
                        row.get("world_name")
                            .or_else(|| row.get("id"))
                            .and_then(Value::as_str)
                            .is_some_and(|name| name.eq_ignore_ascii_case(&id))
                    })
                })
                .cloned()
                .ok_or_else(|| {
                    ApiError(StatusCode::NOT_FOUND, "World is no longer available".into())
                })?;
            Ok(json!({"data":place}))
        },
    )
    .await?;
    Ok(Json(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn place_ids_accept_worlds_without_accepting_urls_or_paths() {
        assert!(valid_place_id("tophub.dcl.eth"));
        assert!(valid_place_id("77777777-7777-4777-8777-777777777777"));
        for id in [
            "https://evil.example",
            "../world.dcl.eth",
            "world.dcl.eth?admin=1",
            "",
            "-world.eth",
        ] {
            assert!(!valid_place_id(id));
        }
    }
    #[test]
    fn multipart_thumbnail_preserves_binary_and_closing_boundary() {
        let mut image = b"\x89PNG\r\n\x1a\n".to_vec();
        image.resize(1024, 255);
        let encoded = STANDARD.encode(&image);
        let body = "--dcl-social-id\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nBuilders\r\n--dcl-social-id--\r\n";
        let result = multipart(body, "id", Some(&encoded)).unwrap();
        assert!(result.windows(image.len()).any(|part| part == image));
        assert!(result.ends_with(b"\r\n--dcl-social-id--\r\n"));
        assert!(result
            .windows(b"name=\"thumbnail".len())
            .any(|part| part == b"name=\"thumbnail"));
        assert_eq!(multipart(body, "id", None).unwrap(), body.as_bytes());
        assert!(multipart(body, "other", Some(&encoded)).is_err());
    }
    #[test]
    fn thumbnail_rejects_unbounded_or_non_raster_inputs() {
        assert!(validate_thumbnail(&STANDARD.encode(b"<svg onload='alert(1)'>")).is_err());
        assert!(validate_thumbnail("not base64!").is_err());
        let mut image = b"\x89PNG\r\n\x1a\n".to_vec();
        image.resize(500 * 1024 + 1, 0);
        assert!(validate_thumbnail(&STANDARD.encode(image)).is_err());
    }
}
