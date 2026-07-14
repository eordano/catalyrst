use std::io::Read;

/// Content addressing is over the DECODED file bytes, but some peers (peer.dclnodes.io) serve
/// content-addressed blobs with `Content-Encoding: gzip` regardless of Accept-Encoding, and
/// this workspace's reqwest has every automatic-decompression feature disabled -- the sync code
/// receives the wire bytes as sent. The transfer coding must therefore be undone here, before
/// hashing and storing, or verification fails on every such blob. `cap` bounds the decoded
/// size so a compression bomb cannot outgrow the caller's body cap.
pub(crate) fn decode_content_encoding(
    encoding: Option<&str>,
    body: Vec<u8>,
    cap: usize,
) -> Result<Vec<u8>, String> {
    match encoding.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "" | "identity" => Ok(body),
        "gzip" | "x-gzip" => {
            let mut decoded = Vec::new();
            flate2::read::MultiGzDecoder::new(body.as_slice())
                .take(cap as u64 + 1)
                .read_to_end(&mut decoded)
                .map_err(|e| format!("gzip transfer decoding failed: {e}"))?;
            if decoded.len() > cap {
                return Err(format!("decoded body exceeds {cap} byte cap"));
            }
            Ok(decoded)
        }
        other => Err(format!("unsupported content-encoding {other:?}")),
    }
}

pub(crate) fn response_content_encoding(resp: &reqwest::Response) -> Option<String> {
    resp.headers()
        .get(reqwest::header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

#[cfg(test)]
pub(crate) fn gzip(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_and_missing_encodings_pass_through() {
        let body = b"raw bytes".to_vec();
        assert_eq!(
            decode_content_encoding(None, body.clone(), 1024).unwrap(),
            body
        );
        assert_eq!(
            decode_content_encoding(Some("identity"), body.clone(), 1024).unwrap(),
            body
        );
        assert_eq!(
            decode_content_encoding(Some(" Identity "), body.clone(), 1024).unwrap(),
            body
        );
    }

    #[test]
    fn gzip_and_x_gzip_decode_to_the_original_bytes() {
        let original = b"### Decentraland json snapshot\n{\"entityId\":\"x\"}\n".to_vec();
        let wire = gzip(&original);
        assert_eq!(
            decode_content_encoding(Some("gzip"), wire.clone(), 1024).unwrap(),
            original
        );
        assert_eq!(
            decode_content_encoding(Some("X-Gzip"), wire, 1024).unwrap(),
            original
        );
    }

    #[test]
    fn non_gzip_bytes_labeled_gzip_fail_decode() {
        let err = decode_content_encoding(Some("gzip"), b"not gzip at all".to_vec(), 1024)
            .expect_err("mislabeled body must be rejected, never hashed as-is");
        assert!(err.contains("gzip transfer decoding failed"), "{err}");
    }

    #[test]
    fn unsupported_encoding_is_rejected() {
        for enc in ["br", "zstd", "deflate", "gzip, br"] {
            let err = decode_content_encoding(Some(enc), b"whatever".to_vec(), 1024)
                .expect_err("unsupported coding must be rejected, never hashed as-is");
            assert!(err.contains("unsupported content-encoding"), "{err}");
        }
    }

    #[test]
    fn decoded_size_cap_is_enforced() {
        let wire = gzip(&vec![0u8; 4096]);
        let err = decode_content_encoding(Some("gzip"), wire, 100)
            .expect_err("a compression bomb must not outgrow the caller's cap");
        assert!(err.contains("exceeds 100 byte cap"), "{err}");
    }
}
