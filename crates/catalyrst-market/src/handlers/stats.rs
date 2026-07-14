use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::http::response::ApiError;
use crate::ports::stats::{parse_category, parse_filters, parse_stat};
use crate::AppState;

const MAX_AGE: u64 = 3600;

pub async fn get_stats(
    State(state): State<AppState>,
    Path((category, stat)): Path<(String, String)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Response, ApiError> {
    let cat = parse_category(&category);
    let st = parse_stat(&stat);
    let filters = parse_filters(&pairs)?;
    let data = state.stats.fetch(cat, st, &filters).await?;

    let (etag_value, body) = encode_stats_etag_and_body(&data);
    let content_length = body.len();

    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        format!("public,max-age={MAX_AGE},s-maxage={MAX_AGE}")
            .parse()
            .unwrap(),
    );
    headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert(LAST_MODIFIED, httpdate_now().parse().unwrap());
    if let Ok(v) = etag_value.parse() {
        headers.insert(ETAG, v);
    }
    headers.insert(CONTENT_LENGTH, content_length.to_string().parse().unwrap());

    Ok((headers, body).into_response())
}

fn encode_stats_etag_and_body<T: serde::Serialize>(data: &T) -> (String, Vec<u8>) {
    match serde_json::to_string(data) {
        Ok(s) => {
            let etag = format!("W/\"{}-{:x}\"", s.len(), fnv1a(&s));
            let mut body = Vec::with_capacity(s.len() + 9);
            body.extend_from_slice(b"{\"data\":");
            body.extend_from_slice(s.as_bytes());
            body.push(b'}');
            (etag, body)
        }
        Err(_) => {
            let etag = format!("W/\"{}-{:x}\"", "null".len(), fnv1a("null"));
            (etag, Vec::new())
        }
    }
}

fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn httpdate_now() -> String {
    use chrono::Utc;
    Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

#[cfg(test)]
mod tests {
    use super::{encode_stats_etag_and_body, fnv1a};
    use crate::ports::prices::NumericKey;
    use crate::ports::stats::{StatsEnvelope, StatsResponse};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct CountingPayload(serde_json::Value, Arc<AtomicUsize>);

    impl serde::Serialize for CountingPayload {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            self.1.fetch_add(1, Ordering::SeqCst);
            self.0.serialize(s)
        }
    }

    #[test]
    fn stats_payload_is_encoded_exactly_once_and_wire_identical() {
        // (1) exactly one serde encode of the payload per call.
        let ctr = Arc::new(AtomicUsize::new(0));
        let fixture = serde_json::json!({ "0": 12, "5000000000000000000": 3 });
        let _ = encode_stats_etag_and_body(&CountingPayload(fixture, ctr.clone()));
        assert_eq!(
            ctr.load(Ordering::SeqCst),
            1,
            "payload encoded exactly once"
        );

        let empty_ctr = Arc::new(AtomicUsize::new(0));
        let _ =
            encode_stats_etag_and_body(&CountingPayload(serde_json::json!({}), empty_ctr.clone()));
        assert_eq!(empty_ctr.load(Ordering::SeqCst), 1);

        // (2) wire parity vs the old two-encode output, for populated and empty maps.
        for pairs in [vec![("0", 12i64), ("5000000000000000000", 3)], vec![]] {
            let mut data: StatsResponse = StatsResponse::new();
            for (k, v) in pairs {
                data.insert(NumericKey(k.to_string()), v);
            }
            let (etag, body) = encode_stats_etag_and_body(&data);

            let expected_body = serde_json::to_vec(&StatsEnvelope { data: data.clone() }).unwrap();
            assert_eq!(body, expected_body, "envelope body byte-identical");

            let s = serde_json::to_string(&data).unwrap();
            let expected_etag = format!("W/\"{}-{:x}\"", s.len(), fnv1a(&s));
            assert_eq!(
                etag, expected_etag,
                "etag byte-identical to bare-data encode"
            );
        }
    }
}
