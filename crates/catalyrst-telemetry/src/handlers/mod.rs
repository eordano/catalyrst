pub mod admin;
pub mod dashboard;
pub mod segment;
pub mod sentry;
pub mod ssr;

use axum::body::Bytes;
use flate2::read::{GzDecoder, ZlibDecoder};
use std::io::Read;

const MAX_DECODED_BYTES: u64 = 16 * 1024 * 1024;

pub fn decode_body(headers: &axum::http::HeaderMap, body: Bytes) -> Vec<u8> {
    let encoding = headers
        .get(axum::http::header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    match encoding {
        "gzip" => {
            let mut out = Vec::new();
            let _ = GzDecoder::new(&body[..])
                .take(MAX_DECODED_BYTES)
                .read_to_end(&mut out);
            out
        }
        "deflate" => {
            let mut out = Vec::new();
            let _ = ZlibDecoder::new(&body[..])
                .take(MAX_DECODED_BYTES)
                .read_to_end(&mut out);
            out
        }
        _ => body.to_vec(),
    }
}
