use axum::extract::Path;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

fn font_bytes(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "archivo-400.woff2" => include_bytes!("../fonts/archivo-400.woff2").as_slice(),
        "archivo-500.woff2" => include_bytes!("../fonts/archivo-500.woff2").as_slice(),
        "archivo-600.woff2" => include_bytes!("../fonts/archivo-600.woff2").as_slice(),
        "archivo-700.woff2" => include_bytes!("../fonts/archivo-700.woff2").as_slice(),
        "archivo-800.woff2" => include_bytes!("../fonts/archivo-800.woff2").as_slice(),
        "ibm-plex-mono-400.woff2" => include_bytes!("../fonts/ibm-plex-mono-400.woff2").as_slice(),
        "ibm-plex-mono-500.woff2" => include_bytes!("../fonts/ibm-plex-mono-500.woff2").as_slice(),
        "ibm-plex-mono-600.woff2" => include_bytes!("../fonts/ibm-plex-mono-600.woff2").as_slice(),
        _ => return None,
    })
}

pub async fn serve(Path(name): Path<String>) -> Response {
    match font_bytes(&name) {
        Some(bytes) => (
            [
                (header::CONTENT_TYPE, HeaderValue::from_static("font/woff2")),
                (
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=31536000, immutable"),
                ),
            ],
            bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAMILIES: &[&str] = &[
        "archivo-400.woff2",
        "archivo-500.woff2",
        "archivo-600.woff2",
        "archivo-700.woff2",
        "archivo-800.woff2",
        "ibm-plex-mono-400.woff2",
        "ibm-plex-mono-500.woff2",
        "ibm-plex-mono-600.woff2",
    ];

    #[test]
    fn every_face_present_and_is_woff2() {
        for name in FAMILIES {
            let bytes = font_bytes(name).unwrap_or_else(|| panic!("missing {name}"));
            assert!(bytes.len() > 1024, "{name} suspiciously small");
            assert_eq!(&bytes[0..4], b"wOF2", "{name} is not a woff2 file");
        }
    }

    #[test]
    fn unknown_name_is_none() {
        assert!(font_bytes("archivo-999.woff2").is_none());
        assert!(font_bytes("../lib.rs").is_none());
    }
}
