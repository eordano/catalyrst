use super::*;
use axum::body::Bytes;
use std::{collections::HashMap, sync::OnceLock, time::Instant};

type Cached = (Instant, &'static str, Bytes);
static CACHE: OnceLock<Mutex<HashMap<String, Cached>>> = OnceLock::new();
// Scene thumbnails can exceed 2 MiB (for example CozyFarm's 2.31 MB PNG).
const MAX_BYTES: usize = 8 * 1024 * 1024;
#[derive(Deserialize)]
pub struct ImageQuery {
    url: String,
}
fn image_url(raw: &str) -> ApiResult<String> {
    let url = reqwest::Url::parse(raw).map_err(|_| bad("Invalid image URL"))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(bad("Unsupported image URL"));
    }
    let path: Vec<_> = url.path().split('/').filter(|s| !s.is_empty()).collect();
    let hash =
        |s: &str| (32..=100).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric());
    let target = match (url.host_str(), path.as_slice()) {
        (
            Some("cdn.decentraland.org" | "assets-cdn.decentraland.org"),
            ["social", "communities", id, "raw-thumbnail.png"],
        ) if Uuid::parse_str(id).is_ok() => {
            format!("https://assets-cdn.decentraland.org/social/communities/{id}/raw-thumbnail.png")
        }
        (Some("profile-images.decentraland.org"), ["entities", id, file])
            if hash(id) && matches!(*file, "face.png" | "body.png") =>
        {
            format!("https://profile-images.decentraland.org/entities/{id}/{file}")
        }
        (
            Some("profile-images-bucket-43d0c58.s3.us-east-1.amazonaws.com"),
            ["v1", "entities", id, file],
        ) if hash(id) && matches!(*file, "face.png" | "body.png") => {
            format!("https://profile-images-bucket-43d0c58.s3.us-east-1.amazonaws.com/v1/entities/{id}/{file}")
        }
        (
            Some(
                "peer.decentraland.org"
                | "peer-ec1.decentraland.org"
                | "peer-ec2.decentraland.org"
                | "peer-wc1.decentraland.org"
                | "peer-eu1.decentraland.org"
                | "peer-ap1.decentraland.org",
            ),
            ["content", "contents", id],
        ) if hash(id) => format!("https://peer.decentraland.org/content/contents/{id}"),
        (Some("worlds-content-server.decentraland.org"), ["contents", id]) if hash(id) => {
            format!("https://worlds-content-server.decentraland.org/contents/{id}")
        }
        (Some("events-assets-099ac00.decentraland.org"), [folder, file])
            if matches!(*folder, "poster" | "poster-vertical")
                && file.rsplit_once('.').is_some_and(|(id, ext)| {
                    (Uuid::parse_str(id).is_ok()
                        || ((16..=64).contains(&id.len())
                            && id.bytes().all(|b| b.is_ascii_hexdigit())))
                        && matches!(ext, "png" | "jpg" | "jpeg" | "webp")
                }) =>
        {
            format!("https://events-assets-099ac00.decentraland.org/{folder}/{file}")
        }
        _ => return Err(bad("Unsupported image source")),
    };
    Ok(target)
}
pub(crate) fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}
fn response(kind: &'static str, bytes: Bytes) -> Response {
    (
        [
            ("content-type", kind),
            ("cache-control", "public, max-age=3600"),
            ("x-content-type-options", "nosniff"),
        ],
        bytes,
    )
        .into_response()
}
pub async fn get_image(
    State(s): State<Arc<Store>>,
    Query(q): Query<ImageQuery>,
) -> ApiResult<Response> {
    let url = image_url(&q.url)?;
    let cache = CACHE.get_or_init(Default::default);
    if let Some((time, kind, bytes)) = cache.lock().unwrap().get(&url) {
        if time.elapsed() < Duration::from_secs(3600) {
            return Ok(response(kind, bytes.clone()));
        }
    }
    let mut upstream = s.client.get(&url).send().await.map_err(|_| unavailable())?;
    // Every redirect must pass the same fixed origin/path allowlist.
    for _ in 0..2 {
        if !upstream.status().is_redirection() {
            break;
        }
        let target = upstream
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(unavailable)?;
        let target = image_url(target)?;
        upstream = s
            .client
            .get(target)
            .send()
            .await
            .map_err(|_| unavailable())?;
    }
    if upstream.status() == StatusCode::NOT_FOUND {
        return Err(ApiError(StatusCode::NOT_FOUND, "Image unavailable".into()));
    }
    if !upstream.status().is_success() {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = upstream.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err(bad("Image is too large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let kind = image_type(&bytes).ok_or_else(|| bad("Unsupported image format"))?;
    let bytes = Bytes::from(bytes);
    let mut cache = cache.lock().unwrap();
    // At most 32 MiB, with oldest-entry eviction; no persistent media copy.
    while cache.values().map(|(_, _, b)| b.len()).sum::<usize>() + bytes.len() > 32 * 1024 * 1024
        || cache.len() >= 128
    {
        let oldest = cache
            .iter()
            .min_by_key(|(_, (time, _, _))| time)
            .map(|(key, _)| key.clone());
        if let Some(key) = oldest {
            cache.remove(&key);
        } else {
            break;
        }
    }
    cache.insert(url, (Instant::now(), kind, bytes.clone()));
    Ok(response(kind, bytes))
}
pub(crate) fn invalidate_community(id: &str) {
    if let Some(cache) = CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.remove(&format!(
                "https://assets-cdn.decentraland.org/social/communities/{id}/raw-thumbnail.png"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_sources_are_fixed_and_content_is_raster_only() {
        let id = "e99471aa-31c4-4952-abf6-99905445f43b";
        assert_eq!(
            image_url(&format!(
                "https://cdn.decentraland.org/social/communities/{id}/raw-thumbnail.png"
            ))
            .unwrap(),
            format!(
                "https://assets-cdn.decentraland.org/social/communities/{id}/raw-thumbnail.png"
            )
        );
        for url in [
            "http://127.0.0.1/a",
            "https://assets-cdn.decentraland.org.evil/a",
            "https://assets-cdn.decentraland.org/admin",
            "https://profile-images.decentraland.org/entities/../private",
            "https://user@peer.decentraland.org/a",
        ] {
            assert!(image_url(url).is_err(), "{url}");
        }
        assert!(image_url("https://profile-images-bucket-43d0c58.s3.us-east-1.amazonaws.com/v1/entities/bafkreifkus6iegxyerri7rbfzmxrj5c726prza7lqobufstyc54yuzxwpy/face.png").is_ok());
        assert!(image_url("https://evil.s3.us-east-1.amazonaws.com/v1/entities/bafkreifkus6iegxyerri7rbfzmxrj5c726prza7lqobufstyc54yuzxwpy/face.png").is_err());
        assert!(image_url("https://worlds-content-server.decentraland.org/contents/bafybeidm7nyllf7d5j4nvdrtvggk2nyizvwtccqzvpztc7fhmyjxsaouza").is_ok());
        assert!(image_url("https://events-assets-099ac00.decentraland.org/poster/0cf95790-55bd-4ab0-9889-197cb3c4614f.webp").is_ok());
        assert!(image_url(
            "https://events-assets-099ac00.decentraland.org/poster/1ad0d5dd69fe2e22.png"
        )
        .is_ok());
        assert!(image_url("https://events-assets-099ac00.decentraland.org/poster/0cf95790-55bd-4ab0-9889-197cb3c4614f.svg").is_err());
        assert_eq!(image_type(b"<svg onload='alert(1)'>"), None);
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
    }
}
