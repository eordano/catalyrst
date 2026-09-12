use anyhow::{anyhow, Result};
use futures::StreamExt;

/// The advertised `Content-Length` is checked first so an oversized body costs nothing,
/// but the running total is enforced too: a chunked or lying upstream never gets to grow
/// the buffer past the cap.
pub async fn read_body_capped(resp: reqwest::Response, max_bytes: usize) -> Result<Vec<u8>> {
    if let Some(len) = resp.content_length() {
        if len > max_bytes as u64 {
            return Err(anyhow!(
                "response body advertises {len} bytes, over the {max_bytes} byte cap"
            ));
        }
    }
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if buf.len().saturating_add(chunk.len()) > max_bytes {
            return Err(anyhow!("response body exceeds the {max_bytes} byte cap"));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sized(body: &'static str) -> reqwest::Response {
        reqwest::Response::from(http::Response::new(body.to_string()))
    }

    fn chunked(chunks: Vec<&'static str>) -> reqwest::Response {
        let stream = futures::stream::iter(
            chunks
                .into_iter()
                .map(|c| Ok::<Vec<u8>, std::io::Error>(c.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
        );
        reqwest::Response::from(http::Response::new(reqwest::Body::wrap_stream(stream)))
    }

    #[tokio::test]
    async fn reads_a_body_under_the_cap() {
        let out = read_body_capped(sized("hello"), 64).await.unwrap();
        assert_eq!(out, b"hello");
    }

    #[tokio::test]
    async fn advertised_length_over_the_cap_fails_before_reading() {
        let err = read_body_capped(sized("hello world"), 4)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("advertises"), "{err}");
    }

    #[tokio::test]
    async fn streamed_body_over_the_cap_fails_mid_read() {
        let err = read_body_capped(chunked(vec!["aaaa", "bbbb", "cccc"]), 6)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("exceeds"), "{err}");
    }

    #[tokio::test]
    async fn streamed_body_under_the_cap_is_concatenated() {
        let out = read_body_capped(chunked(vec!["aa", "bb", "cc"]), 6)
            .await
            .unwrap();
        assert_eq!(out, b"aabbcc");
    }

    #[tokio::test]
    async fn a_body_exactly_at_the_cap_is_accepted() {
        let out = read_body_capped(sized("hello"), 5).await.unwrap();
        assert_eq!(out, b"hello");
    }
}
