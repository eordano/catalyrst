use axum::extract::{OriginalUri, Path, State};
use axum::http::{HeaderMap, Method};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::auth_address_verified;
use crate::auth_chain::AUTH_METADATA_HEADER;
use crate::http::errors::ApiError;
use crate::s3::{
    presign_put_object, PresignPutObject, ReportUploadMode, REPORT_LOCAL_FALLBACK_ENV,
};
use crate::AppState;

fn request_base_url(headers: &HeaderMap) -> String {
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "127.0.0.1:5134".to_string());
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if host.starts_with("127.0.0.1") || host.starts_with("localhost") {
                "http".to_string()
            } else {
                "https".to_string()
            }
        });
    format!("{}://{}", scheme, host)
}

fn is_federation_envelope(body: &Option<Json<Value>>) -> bool {
    body.as_ref()
        .and_then(|Json(v)| v.as_object())
        .map(|o| {
            o.contains_key("domain") && o.contains_key("message") && o.contains_key("signature")
        })
        .unwrap_or(false)
}

/// Object metadata for the S3 PUT, mirroring upstream's
/// `{ ...userAuth.metadata, address: userAuth.address }`. The signed-fetch
/// metadata (the `x-identity-metadata` header JSON) is spread first; `address`
/// is appended last (and so wins on collision, as `{...spread, address}` does
/// in JS). Only string-valued metadata keys are kept (S3 metadata is string).
fn report_metadata(headers: &HeaderMap, address: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    if let Some(raw) = headers
        .get(AUTH_METADATA_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(raw) {
            for (k, v) in map {
                if k == "address" {
                    continue;
                }
                if let Some(s) = v.as_str() {
                    out.push((k, s.to_string()));
                }
            }
        }
    }
    out.push(("address".to_string(), address.to_string()));
    out
}

/// Build the upstream filename: `address.slice(-8).toLowerCase() +
/// floor(now_ms/1000).toString(16) + ".json"`.
fn report_filename(address: &str, now_seconds: i64) -> String {
    let user_hash: String = {
        let chars: Vec<char> = address.chars().collect();
        let start = chars.len().saturating_sub(8);
        chars[start..].iter().collect::<String>().to_lowercase()
    };
    let time_hash = format!("{:x}", now_seconds);
    format!("{}{}.json", user_hash, time_hash)
}

pub async fn post_report(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    // Signed federation report (places.md §3): Signed<PlaceReport> envelope ->
    // verify -> replay -> log -> gossip. Advisory-only per ADR.
    if is_federation_envelope(&body) {
        return crate::handlers::federation::fed_post_report(&state, &headers, &body).await;
    }
    let user = auth_address_verified(&headers, method.as_str(), uri.path())?;
    let payload = body.map(|Json(v)| v).unwrap_or_else(|| json!({}));
    let entity_id = payload
        .get("entity_id")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("place_id").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    let now = chrono::Utc::now();
    let filename = report_filename(&user, now.timestamp());

    // Upstream mints a real AWS S3 presigned PUT URL via aws-sdk-js v2
    // `s3.getSignedUrl("putObject", ...)` (signatureVersion "s3" == SigV2:
    // AWSAccessKeyId + absolute Expires + base64 Signature + the request headers
    // folded into query params; 60000s expiry, ContentType application/json, ACL
    // private, CacheControl, object Metadata). When the bucket creds
    // (AWS_ACCESS_KEY / AWS_ACCESS_SECRET / AWS_BUCKET_NAME) are configured we
    // produce a byte-faithful presigned URL and the client uploads the report
    // JSON directly to S3 — this is the upstream-compatible production path,
    // taken whenever the creds are present.
    //
    // Without creds the endpoint is misconfigured for production (upstream has
    // no fallback at all). We fail closed with a 503 unless the operator has
    // *explicitly* opted into the DEV-ONLY same-origin local-upload route via
    // `PLACES_REPORT_LOCAL_FALLBACK`. The local route is never a silent default.
    let signed_url = match ReportUploadMode::from_env() {
        ReportUploadMode::S3(cfg) => {
            let metadata = report_metadata(&headers, &user);
            presign_put_object(
                &cfg,
                &PresignPutObject {
                    key: &filename,
                    content_type: "application/json",
                    acl: "private",
                    cache_control: "public, max-age=31536000, immutable",
                    expires: 60 * 1000,
                    metadata: &metadata,
                },
                now,
            )
        }
        ReportUploadMode::LocalDev => {
            tracing::warn!(
                target: "catalyrst_places::report",
                "DEV-ONLY: AWS_ACCESS_KEY/AWS_ACCESS_SECRET/AWS_BUCKET_NAME unset and \
                 {flag}=true — /api/report is serving a same-origin local-upload URL \
                 instead of the upstream S3 presigned PUT. DO NOT run this in production; \
                 configure AWS_* for upstream parity.",
                flag = REPORT_LOCAL_FALLBACK_ENV,
            );
            format!(
                "{}/api/report/upload/{}",
                request_base_url(&headers),
                filename
            )
        }
        ReportUploadMode::Misconfigured => {
            tracing::error!(
                target: "catalyrst_places::report",
                "/api/report has no S3 bucket configured: set AWS_ACCESS_KEY, \
                 AWS_ACCESS_SECRET and AWS_BUCKET_NAME (see the crate README). \
                 Failing closed with 503. For a no-S3 local-dev loop only, set \
                 {flag}=true to serve a non-upstream local-upload URL instead.",
                flag = REPORT_LOCAL_FALLBACK_ENV,
            );
            return Err(ApiError::service_unavailable(
                "report upload is not configured (S3 bucket credentials missing)",
            ));
        }
    };

    state
        .places
        .record_report(
            entity_id.as_deref(),
            &user,
            &signed_url,
            &filename,
            &payload,
        )
        .await?;

    Ok(Json(
        json!({ "ok": true, "data": { "signed_url": signed_url } }),
    ))
}

pub async fn put_report_upload(
    State(state): State<AppState>,
    Path(filename): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let payload = body.map(|Json(v)| v).unwrap_or_else(|| json!({}));
    state
        .places
        .record_report_upload(&filename, &payload)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    #[test]
    fn filename_matches_upstream() {
        // address.slice(-8).toLowerCase() + floor(ms/1000).toString(16) + ".json"
        let addr = "0x1234567890ABCDEF1234567890abcdef12345678";
        // last 8 chars: "12345678"
        let now_seconds = 0x6500_0000_i64; // hex 65000000
        let f = report_filename(addr, now_seconds);
        assert_eq!(f, "1234567865000000.json");
    }

    #[test]
    fn misconfigured_report_mode_is_fail_closed_503() {
        // The branch the handler takes when no creds + no local-fallback opt-in:
        // it must produce a 503 ServiceUnavailable, never a local-upload URL.
        let mode = ReportUploadMode::resolve(None, None);
        assert!(matches!(mode, ReportUploadMode::Misconfigured));
        let err = ApiError::service_unavailable(
            "report upload is not configured (S3 bucket credentials missing)",
        );
        let resp = err.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn metadata_spreads_then_address() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_METADATA_HEADER,
            r#"{"signer":"dcl:explorer","intent":"report"}"#.parse().unwrap(),
        );
        let md = report_metadata(&headers, "0xabc");
        // signer + intent + address (string-only, address last)
        assert!(md.contains(&("signer".to_string(), "dcl:explorer".to_string())));
        assert!(md.contains(&("intent".to_string(), "report".to_string())));
        assert_eq!(
            md.last().unwrap(),
            &("address".to_string(), "0xabc".to_string())
        );
    }
}
