//! One policy for what a world setting may contain, shared by PUT
//! /world/:name/settings (which rejects violations) and the deploy path (which
//! treats them as "not expressed" so an otherwise valid deployment survives).

use tokio::io::AsyncReadExt;

pub const VALID_RATINGS: [&str; 5] = ["RP", "E", "T", "A", "R"];
pub const TITLE_MIN_LENGTH: usize = 3;
pub const TITLE_MAX_LENGTH: usize = 100;
pub const DESCRIPTION_MIN_LENGTH: usize = 3;
pub const DESCRIPTION_MAX_LENGTH: usize = 1000;
pub const MAX_CATEGORIES: usize = 20;

/// Length bounds are JS string `.length` -- UTF-16 code units -- so multi-byte
/// text measures the same on both sides of the port.
pub fn text_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// JS truthiness for deployer-supplied JSON: `false`, `null`, `0`/`-0` and
/// `""` are falsy; everything else (objects, arrays, non-empty strings, other
/// numbers) is truthy.
pub fn js_truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => true,
    }
}

/// worlds.skybox_time is an INTEGER column; only a finite, integral number in
/// i32 range is storable (Number.isInteger upstream, so integral floats pass).
/// Callers refuse or drop anything else instead of wrapping, flooring, or
/// surfacing a raw cast error.
pub fn storable_skybox_time(n: f64) -> Option<i32> {
    if !n.is_finite() || n.fract() != 0.0 {
        return None;
    }
    i32::try_from(n as i64).ok()
}

/// Longest magic-byte signature checked below (RIFF....WEBP).
pub const THUMBNAIL_SIGNATURE_BYTES: usize = 12;

/// Thumbnails are stored and later served verbatim, so anything that is not a
/// real raster image (e.g. HTML/SVG/scripts smuggled as a "thumbnail") must be
/// rejected.
pub fn detect_image_format(buf: &[u8]) -> Option<&'static str> {
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

/// Keeps a scene's navmapThumbnail hash only when its stored bytes are one of
/// the image formats the settings endpoint accepts, so a deploy cannot promote
/// a non-image file into world settings. An unreadable or unsupported file is
/// treated as no thumbnail, never as a deployment failure.
pub async fn storable_thumbnail_hash(contents_dir: &std::path::Path, hash: &str) -> Option<String> {
    let mut file = match tokio::fs::File::open(contents_dir.join(hash)).await {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(hash = %hash, error = %e, "could not verify the scene thumbnail; storing the world without it");
            return None;
        }
    };
    let mut signature = [0u8; THUMBNAIL_SIGNATURE_BYTES];
    let mut read = 0;
    while read < THUMBNAIL_SIGNATURE_BYTES {
        match file.read(&mut signature[read..]).await {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(e) => {
                tracing::warn!(hash = %hash, error = %e, "could not verify the scene thumbnail; storing the world without it");
                return None;
            }
        }
    }
    if detect_image_format(&signature[..read]).is_some() {
        Some(hash.to_string())
    } else {
        tracing::info!(hash = %hash, "ignoring scene thumbnail that is not a supported image");
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_len_counts_utf16_code_units() {
        assert_eq!(text_len("abc"), 3);
        assert_eq!(text_len("\u{65E5}\u{672C}"), 2);
        assert_eq!(text_len("\u{1D11E}"), 2);
        assert_eq!(text_len(""), 0);
    }

    #[test]
    fn js_truthy_matches_javascript() {
        use serde_json::json;
        assert!(!js_truthy(&json!(false)));
        assert!(!js_truthy(&json!(null)));
        assert!(!js_truthy(&json!(0)));
        assert!(!js_truthy(&json!(-0.0)));
        assert!(!js_truthy(&json!("")));
        assert!(js_truthy(&json!(true)));
        assert!(js_truthy(&json!(1)));
        assert!(js_truthy(&json!("false")));
        assert!(js_truthy(&json!("yes")));
        assert!(js_truthy(&json!({})));
        assert!(js_truthy(&json!([])));
    }

    #[test]
    fn storable_skybox_time_requires_a_finite_integral_i32() {
        assert_eq!(storable_skybox_time(36000.0), Some(36000));
        assert_eq!(storable_skybox_time(-2147483648.0), Some(i32::MIN));
        assert_eq!(storable_skybox_time(2147483647.0), Some(i32::MAX));
        assert_eq!(storable_skybox_time(1.5), None);
        assert_eq!(storable_skybox_time(3000000000.0), None);
        assert_eq!(storable_skybox_time(-2147483649.0), None);
        assert_eq!(storable_skybox_time(f64::NAN), None);
        assert_eq!(storable_skybox_time(f64::INFINITY), None);
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
        assert!(detect_image_format(b"GIF8XX").is_none());
    }

    #[tokio::test]
    async fn storable_thumbnail_hash_checks_stored_bytes() {
        let dir = std::env::temp_dir().join(format!(
            "worlds-thumb-policy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let png = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        tokio::fs::write(dir.join("bafyimage"), png).await.unwrap();
        tokio::fs::write(dir.join("bafyhtml"), b"<script>alert(1)</script>")
            .await
            .unwrap();

        assert_eq!(
            storable_thumbnail_hash(&dir, "bafyimage").await.as_deref(),
            Some("bafyimage")
        );
        assert_eq!(storable_thumbnail_hash(&dir, "bafyhtml").await, None);
        assert_eq!(storable_thumbnail_hash(&dir, "bafymissing").await, None);

        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
