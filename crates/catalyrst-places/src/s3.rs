//! AWS S3 query-string presigning for `putObject`, byte-faithful to the
//! presigned PUT URL that upstream `places/src/entities/Report/routes.ts`
//! actually emits.
//!
//! Upstream constructs `new AWS.S3({ accessKeyId, secretAccessKey })` and calls
//! `s3.getSignedUrl("putObject", {...})`. With **aws-sdk-js v2** that S3 client
//! has `signatureVersion: "s3"`, i.e. it signs with **AWS Signature Version 2**
//! (HMAC-SHA1, the legacy S3 query-auth scheme) — *not* SigV4. The resulting URL
//! carries `AWSAccessKeyId`, an absolute-epoch `Expires`, a base64 `Signature`,
//! and the request headers (`Content-Type`, `Cache-Control`, `x-amz-acl`,
//! `x-amz-meta-*`) folded back in as plain query params. This module reproduces
//! that exact wire (verified byte-for-byte against aws-sdk-js v2 in the tests).
//!
//! ## Required environment (production / S3 path)
//!
//! The presigned-PUT path activates iff **all three** are set (upstream reads
//! the same `AWS_ACCESS_KEY` / `AWS_ACCESS_SECRET` / `AWS_BUCKET_NAME`):
//!
//! | var                 | upstream name        | meaning                                   |
//! |---------------------|----------------------|-------------------------------------------|
//! | `AWS_ACCESS_KEY`    | `AWS_ACCESS_KEY`     | IAM access key id                         |
//! | `AWS_ACCESS_SECRET` | `AWS_ACCESS_SECRET`  | IAM secret access key                     |
//! | `AWS_BUCKET_NAME`   | `AWS_BUCKET_NAME`    | target bucket                             |
//! | `BUCKET_HOSTNAME`   | `BUCKET_HOSTNAME`    | optional CDN/proxy host substituted for the S3 host |
//! | `AWS_REGION`        | `AWS_REGION`*        | optional; selects the regional S3 host (default `us-east-1`) |
//! | `AWS_ENDPOINT`      | (catalyrst-only)     | optional S3-compatible endpoint (MinIO/localstack); forces path-style |
//!
//! \* aws-sdk-js v2 reads `AWS_REGION` into its global config, which the report
//! S3 client inherits; SigV2 signs over `/bucket/key` so the region only changes
//! the host, never the signature. `AWS_ENDPOINT` is a catalyrst-local escape
//! hatch for S3-compatible stores (upstream's report client never reads it); it
//! only affects behavior when explicitly set.
//!
//! ## Production vs dev: what happens when the creds are unset
//!
//! Upstream has no fallback at all — `POST /report` *always* presigns against
//! S3, so a deploy with the creds unset is a misconfiguration there too. To
//! keep that production posture while still allowing a no-S3 local-dev loop, the
//! report path resolves to one of three [`ReportUploadMode`]s:
//!
//! | creds set | `PLACES_REPORT_LOCAL_FALLBACK` | mode | behavior |
//! |-----------|--------------------------------|------|----------|
//! | yes       | (ignored)                      | [`ReportUploadMode::S3`]      | byte-faithful presigned PUT (the upstream production wire) |
//! | no        | `true`/`1`/`yes`/`on`          | [`ReportUploadMode::LocalDev`] | same-origin local-upload URL, **dev-only** (logged WARN) |
//! | no        | unset / false                  | [`ReportUploadMode::Misconfigured`] | `POST /report` fails 503 — fail-closed, not a silent degrade |
//!
//! So the SigV4/SigV2 path is taken **whenever creds are configured**, and the
//! non-upstream local route is never a silent default: an operator has to opt in
//! to it explicitly with `PLACES_REPORT_LOCAL_FALLBACK`. See the crate
//! `README.md` for the full env contract.

use base64::Engine;
use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// Configuration for the S3 bucket the report endpoint presigns against.
/// Mirrors upstream's `AWS_ACCESS_KEY / AWS_ACCESS_SECRET / AWS_BUCKET_NAME /
/// BUCKET_HOSTNAME / AWS_REGION / AWS_ENDPOINT` env. `region` defaults to
/// `us-east-1` (the aws-sdk-js default when unset).
#[derive(Debug, Clone)]
pub struct S3Config {
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    /// Optional CDN/proxy host substituted for the S3 host (BUCKET_HOSTNAME).
    pub bucket_hostname: Option<String>,
    pub region: String,
    /// Optional custom endpoint (e.g. MinIO/localstack); forces path-style.
    pub endpoint: Option<String>,
}

impl S3Config {
    /// Load from the same env vars upstream reads. Returns `None` when the
    /// access key / secret / bucket are not all configured (so the caller can
    /// fall back to the local-upload URL on a no-S3 deploy).
    pub fn from_env() -> Option<Self> {
        let access_key = non_empty(std::env::var("AWS_ACCESS_KEY").ok())?;
        let secret_key = non_empty(std::env::var("AWS_ACCESS_SECRET").ok())?;
        let bucket = non_empty(std::env::var("AWS_BUCKET_NAME").ok())?;
        Some(Self {
            access_key,
            secret_key,
            bucket,
            bucket_hostname: non_empty(std::env::var("BUCKET_HOSTNAME").ok()),
            region: non_empty(std::env::var("AWS_REGION").ok())
                .unwrap_or_else(|| "us-east-1".to_string()),
            endpoint: non_empty(std::env::var("AWS_ENDPOINT").ok()),
        })
    }
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// How `POST /report` should mint its `signed_url`, resolved once from env.
///
/// Computed by [`ReportUploadMode::from_env`] so the handler never re-derives
/// the policy. See the module docs for the truth table.
#[derive(Debug, Clone)]
pub enum ReportUploadMode {
    /// Bucket creds are configured: presign a real S3 PUT (the upstream wire).
    S3(S3Config),
    /// No creds, but `PLACES_REPORT_LOCAL_FALLBACK` is opted in: serve a
    /// same-origin local-upload URL. **Dev-only** — not the upstream wire.
    LocalDev,
    /// No creds and no opt-in: the report endpoint is misconfigured for
    /// production and must fail closed (503) rather than silently degrade.
    Misconfigured,
}

/// Name of the explicit opt-in flag that enables the dev-only local fallback.
pub const REPORT_LOCAL_FALLBACK_ENV: &str = "PLACES_REPORT_LOCAL_FALLBACK";

impl ReportUploadMode {
    /// Resolve from the same env [`S3Config::from_env`] reads, plus the
    /// `PLACES_REPORT_LOCAL_FALLBACK` opt-in flag.
    pub fn from_env() -> Self {
        Self::resolve(
            S3Config::from_env(),
            std::env::var(REPORT_LOCAL_FALLBACK_ENV).ok(),
        )
    }

    /// Pure resolver (env-free) so the policy is unit-testable.
    pub fn resolve(s3: Option<S3Config>, fallback_flag: Option<String>) -> Self {
        match s3 {
            Some(cfg) => Self::S3(cfg),
            None if env_flag_enabled(fallback_flag.as_deref()) => Self::LocalDev,
            None => Self::Misconfigured,
        }
    }
}

/// Truthy parse for a boolean env flag: `true`/`1`/`yes`/`on` (case-insensitive).
fn env_flag_enabled(v: Option<&str>) -> bool {
    matches!(
        v.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("true") | Some("1") | Some("yes") | Some("on")
    )
}

/// Parameters for a single presigned `putObject` (the upstream report call).
pub struct PresignPutObject<'a> {
    /// Object key (the filename).
    pub key: &'a str,
    /// `ContentType` request header (upstream: `application/json`).
    pub content_type: &'a str,
    /// `x-amz-acl` (upstream: `private`).
    pub acl: &'a str,
    /// `Cache-Control` (upstream: `public, max-age=31536000, immutable`).
    pub cache_control: &'a str,
    /// Expiry in **seconds** added to the request time to form the absolute
    /// `Expires` epoch (upstream: `60 * 1000` == 60000).
    pub expires: u64,
    /// Object metadata -> emitted as `x-amz-meta-<k>` query params, key
    /// lowercased, value verbatim.
    pub metadata: &'a [(String, String)],
}

/// Produce an AWS Signature Version 2 (S3 query-auth) presigned PUT URL, exactly
/// as aws-sdk-js v2 `s3.getSignedUrl("putObject", {...})` does.
///
/// `now` is the request time; the SigV2 `Expires` is `floor(now)+expires`
/// seconds (absolute epoch).
pub fn presign_put_object(
    cfg: &S3Config,
    req: &PresignPutObject<'_>,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    // host + URL path (canonical_uri). Virtual-hosted-style (`bucket.s3...`)
    // unless a custom endpoint forces path-style (`endpoint/bucket/key`).
    let (host, url_path) = match &cfg.endpoint {
        Some(ep) => {
            let host = endpoint_host(ep);
            (
                host,
                format!(
                    "/{}/{}",
                    uri_escape_path(&cfg.bucket),
                    uri_escape_path(req.key)
                ),
            )
        }
        None => {
            let host = if cfg.region == "us-east-1" {
                format!("{}.s3.amazonaws.com", cfg.bucket)
            } else {
                format!("{}.s3.{}.amazonaws.com", cfg.bucket, cfg.region)
            };
            (host, format!("/{}", uri_escape_path(req.key)))
        }
    };

    // Absolute Expires epoch: floor(now_seconds) + expires (SigV2 presign).
    let expires_abs = now.timestamp() + req.expires as i64;

    // Canonicalized amz headers: every `x-amz-*` request header, sorted by
    // lowercase name, joined as `name:value\n...`. For a presigned putObject the
    // only x-amz-* headers are `x-amz-acl` and the `x-amz-meta-*` metadata.
    let mut amz_headers: Vec<(String, String)> = Vec::new();
    amz_headers.push(("x-amz-acl".to_string(), req.acl.to_string()));
    for (k, v) in req.metadata {
        amz_headers.push((format!("x-amz-meta-{}", k.to_lowercase()), v.clone()));
    }
    amz_headers.sort_by_key(|a| a.0.to_lowercase());
    let canonical_amz_headers = amz_headers
        .iter()
        .map(|(k, v)| format!("{}:{}", k, v))
        .collect::<Vec<_>>()
        .join("\n");

    // Canonicalized resource: aws-sdk-js v2 signs the *escaped* request path
    // (`r.path`). Virtual-hosted-style: `r.path` is `/<escaped-key>` and the
    // SDK prepends `/<bucket>` (the un-escaped bucket name) -> `/<bucket>/<key>`.
    // Path-style (custom endpoint): the bucket is already in `r.path`
    // (`/<escaped-bucket>/<escaped-key>`), so we sign that path verbatim.
    let canonical_resource = match &cfg.endpoint {
        Some(_) => url_path.clone(),
        None => format!("/{}{}", cfg.bucket, url_path),
    };

    // StringToSign = METHOD\nContent-MD5\nContent-Type\nExpires\n
    //                CanonicalizedAmzHeaders\nCanonicalizedResource
    // (Content-MD5 empty; Cache-Control is *not* part of the SigV2 STS.)
    let string_to_sign = format!(
        "PUT\n\n{}\n{}\n{}\n{}",
        req.content_type, expires_abs, canonical_amz_headers, canonical_resource
    );

    let signature = sign_sha1_base64(&cfg.secret_key, string_to_sign.as_bytes());

    // Build the URL query params dict, then sort + uri-escape (aws-sdk-js
    // `queryParamsToString`). Params: AWSAccessKeyId, Cache-Control,
    // Content-Type, Expires, Signature, x-amz-acl, x-amz-meta-*.
    let mut params: Vec<(String, String)> = vec![
        ("AWSAccessKeyId".to_string(), cfg.access_key.clone()),
        ("Cache-Control".to_string(), req.cache_control.to_string()),
        ("Content-Type".to_string(), req.content_type.to_string()),
        ("Expires".to_string(), expires_abs.to_string()),
        ("Signature".to_string(), signature),
        ("x-amz-acl".to_string(), req.acl.to_string()),
    ];
    for (k, v) in req.metadata {
        params.push((format!("x-amz-meta-{}", k.to_lowercase()), v.clone()));
    }
    params.sort_by(|a, b| a.0.cmp(&b.0));
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", uri_escape(k), uri_escape(v)))
        .collect::<Vec<_>>()
        .join("&");

    // Final URL. Host is the bucket (or BUCKET_HOSTNAME override). The signature
    // is independent of host (SigV2 signs `/bucket/key`), so the override only
    // rewrites the host, exactly as upstream's `url.hostname = BUCKET_HOSTNAME`.
    let scheme = url_scheme(cfg);
    let final_host = match &cfg.bucket_hostname {
        Some(h) => h.clone(),
        None => host,
    };
    format!("{}://{}{}?{}", scheme, final_host, url_path, query)
}

fn url_scheme(cfg: &S3Config) -> &'static str {
    match &cfg.endpoint {
        Some(ep) if ep.starts_with("http://") => "http",
        _ => "https",
    }
}

fn endpoint_host(ep: &str) -> String {
    ep.trim_end_matches('/')
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string()
}

fn sign_sha1_base64(secret: &str, data: &[u8]) -> String {
    let mut mac = HmacSha1::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(data);
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

/// RFC3986 percent-encoding matching aws-sdk-js v2 `util.uriEscape`: only
/// `A-Za-z0-9-._~` survive; every other byte becomes `%XX` (uppercase hex).
/// Used for query-param keys and values.
pub fn uri_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push(hex_upper(b >> 4));
                out.push(hex_upper(b & 0x0f));
            }
        }
    }
    out
}

/// Per-segment path escaping (aws-sdk-js `util.uriEscapePath`): split on `/`,
/// `uriEscape` each segment, re-join with `/` so slashes are preserved.
fn uri_escape_path(input: &str) -> String {
    input
        .split('/')
        .map(uri_escape)
        .collect::<Vec<_>>()
        .join("/")
}

fn hex_upper(nibble: u8) -> char {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    HEX[nibble as usize] as char
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn cfg() -> S3Config {
        S3Config {
            access_key: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            bucket: "examplebucket".to_string(),
            bucket_hostname: None,
            region: "us-east-1".to_string(),
            endpoint: None,
        }
    }

    fn now_2013() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2013, 5, 24, 0, 0, 0).unwrap()
    }

    /// Byte-for-byte parity with aws-sdk-js v2 `s3.getSignedUrl("putObject")`.
    /// The expected URL was generated with aws-sdk@2.x against this exact input
    /// (access key/secret = the AWS-published example creds; time frozen to
    /// 2013-05-24T00:00:00Z; Expires=60000; signer+address metadata).
    #[test]
    fn matches_aws_sdk_js_v2_golden_url() {
        let url = presign_put_object(
            &cfg(),
            &PresignPutObject {
                key: "abcd1234deadbeef.json",
                content_type: "application/json",
                acl: "private",
                cache_control: "public, max-age=31536000, immutable",
                expires: 60000,
                metadata: &[
                    ("signer".to_string(), "dcl:explorer".to_string()),
                    ("address".to_string(), "0xabc".to_string()),
                ],
            },
            now_2013(),
        );
        assert_eq!(
            url,
            "https://examplebucket.s3.amazonaws.com/abcd1234deadbeef.json?\
AWSAccessKeyId=AKIAIOSFODNN7EXAMPLE&\
Cache-Control=public%2C%20max-age%3D31536000%2C%20immutable&\
Content-Type=application%2Fjson&\
Expires=1369413600&\
Signature=Br7FhFIMDcrqVxHZYsLZn6OlUC8%3D&\
x-amz-acl=private&\
x-amz-meta-address=0xabc&\
x-amz-meta-signer=dcl%3Aexplorer"
        );
    }

    /// No-metadata variant (only `address`-less). aws-sdk-js v2 golden.
    #[test]
    fn matches_aws_sdk_js_v2_no_metadata() {
        let url = presign_put_object(
            &cfg(),
            &PresignPutObject {
                key: "k.json",
                content_type: "application/json",
                acl: "private",
                cache_control: "public, max-age=31536000, immutable",
                expires: 60000,
                metadata: &[],
            },
            now_2013(),
        );
        assert_eq!(
            url,
            "https://examplebucket.s3.amazonaws.com/k.json?\
AWSAccessKeyId=AKIAIOSFODNN7EXAMPLE&\
Cache-Control=public%2C%20max-age%3D31536000%2C%20immutable&\
Content-Type=application%2Fjson&\
Expires=1369413600&\
Signature=9BfqtWlsn7yRRFxVoAr67qJ30%2Fc%3D&\
x-amz-acl=private"
        );
    }

    /// Key containing a space and a slash. aws-sdk-js v2 golden:
    /// path becomes `/a%20b/c.json` (space escaped, slash preserved).
    #[test]
    fn matches_aws_sdk_js_v2_slash_key() {
        let url = presign_put_object(
            &cfg(),
            &PresignPutObject {
                key: "a b/c.json",
                content_type: "application/json",
                acl: "private",
                cache_control: "public, max-age=31536000, immutable",
                expires: 60000,
                metadata: &[("address".to_string(), "0xABC".to_string())],
            },
            now_2013(),
        );
        assert_eq!(
            url,
            "https://examplebucket.s3.amazonaws.com/a%20b/c.json?\
AWSAccessKeyId=AKIAIOSFODNN7EXAMPLE&\
Cache-Control=public%2C%20max-age%3D31536000%2C%20immutable&\
Content-Type=application%2Fjson&\
Expires=1369413600&\
Signature=ekv0nItlf1Ijov1BJcczWI8SZyk%3D&\
x-amz-acl=private&\
x-amz-meta-address=0xABC"
        );
    }

    /// Non-us-east-1 region: only the host changes; the SigV2 signature is
    /// identical (it signs `/bucket/key`, not the host). aws-sdk-js v2 golden.
    #[test]
    fn matches_aws_sdk_js_v2_regional_host() {
        let mut c = cfg();
        c.region = "eu-west-1".to_string();
        let url = presign_put_object(
            &c,
            &PresignPutObject {
                key: "abcd1234deadbeef.json",
                content_type: "application/json",
                acl: "private",
                cache_control: "public, max-age=31536000, immutable",
                expires: 60000,
                metadata: &[
                    ("signer".to_string(), "dcl:explorer".to_string()),
                    ("address".to_string(), "0xabc".to_string()),
                ],
            },
            now_2013(),
        );
        assert!(url.starts_with(
            "https://examplebucket.s3.eu-west-1.amazonaws.com/abcd1234deadbeef.json?"
        ));
        // identical signature to us-east-1
        assert!(url.ends_with("Signature=Br7FhFIMDcrqVxHZYsLZn6OlUC8%3D&x-amz-acl=private&x-amz-meta-address=0xabc&x-amz-meta-signer=dcl%3Aexplorer"));
    }

    /// BUCKET_HOSTNAME override rewrites only the host (upstream
    /// `url.hostname = BUCKET_HOSTNAME`); signature unchanged.
    #[test]
    fn bucket_hostname_override() {
        let mut c = cfg();
        c.bucket_hostname = Some("reports.decentraland.org".to_string());
        let url = presign_put_object(
            &c,
            &PresignPutObject {
                key: "abcd1234deadbeef.json",
                content_type: "application/json",
                acl: "private",
                cache_control: "public, max-age=31536000, immutable",
                expires: 60000,
                metadata: &[
                    ("signer".to_string(), "dcl:explorer".to_string()),
                    ("address".to_string(), "0xabc".to_string()),
                ],
            },
            now_2013(),
        );
        assert_eq!(
            url,
            "https://reports.decentraland.org/abcd1234deadbeef.json?\
AWSAccessKeyId=AKIAIOSFODNN7EXAMPLE&\
Cache-Control=public%2C%20max-age%3D31536000%2C%20immutable&\
Content-Type=application%2Fjson&\
Expires=1369413600&\
Signature=Br7FhFIMDcrqVxHZYsLZn6OlUC8%3D&\
x-amz-acl=private&\
x-amz-meta-address=0xabc&\
x-amz-meta-signer=dcl%3Aexplorer"
        );
    }

    /// Path-style custom endpoint (MinIO/localstack). aws-sdk-js v2 golden with
    /// `endpoint: "http://localhost:9000", s3ForcePathStyle: true`. Same SigV2
    /// signature (resource `/examplebucket/<key>` is host-independent), http
    /// scheme, and `/<bucket>/<key>` path.
    #[test]
    fn matches_aws_sdk_js_v2_path_style_endpoint() {
        let mut c = cfg();
        c.endpoint = Some("http://localhost:9000".to_string());
        let url = presign_put_object(
            &c,
            &PresignPutObject {
                key: "abcd1234deadbeef.json",
                content_type: "application/json",
                acl: "private",
                cache_control: "public, max-age=31536000, immutable",
                expires: 60000,
                metadata: &[
                    ("signer".to_string(), "dcl:explorer".to_string()),
                    ("address".to_string(), "0xabc".to_string()),
                ],
            },
            now_2013(),
        );
        assert_eq!(
            url,
            "http://localhost:9000/examplebucket/abcd1234deadbeef.json?\
AWSAccessKeyId=AKIAIOSFODNN7EXAMPLE&\
Cache-Control=public%2C%20max-age%3D31536000%2C%20immutable&\
Content-Type=application%2Fjson&\
Expires=1369413600&\
Signature=Br7FhFIMDcrqVxHZYsLZn6OlUC8%3D&\
x-amz-acl=private&\
x-amz-meta-address=0xabc&\
x-amz-meta-signer=dcl%3Aexplorer"
        );
    }

    #[test]
    fn uri_escape_rules() {
        assert_eq!(uri_escape("dcl:explorer"), "dcl%3Aexplorer");
        assert_eq!(
            uri_escape("public, max-age=31536000, immutable"),
            "public%2C%20max-age%3D31536000%2C%20immutable"
        );
        assert_eq!(uri_escape("-._~AZ09"), "-._~AZ09");
        assert_eq!(uri_escape("/"), "%2F");
        assert_eq!(uri_escape("application/json"), "application%2Fjson");
    }

    #[test]
    fn uri_escape_path_preserves_slash() {
        assert_eq!(uri_escape_path("a b/c.json"), "a%20b/c.json");
        assert_eq!(uri_escape_path("k.json"), "k.json");
    }

    #[test]
    fn report_mode_s3_when_creds_present() {
        // Creds configured -> S3 path regardless of the fallback flag.
        assert!(matches!(
            ReportUploadMode::resolve(Some(cfg()), None),
            ReportUploadMode::S3(_)
        ));
        assert!(matches!(
            ReportUploadMode::resolve(Some(cfg()), Some("true".to_string())),
            ReportUploadMode::S3(_)
        ));
    }

    #[test]
    fn report_mode_misconfigured_when_no_creds_and_no_optin() {
        // No creds + no opt-in -> fail-closed, never a silent local fallback.
        assert!(matches!(
            ReportUploadMode::resolve(None, None),
            ReportUploadMode::Misconfigured
        ));
        assert!(matches!(
            ReportUploadMode::resolve(None, Some("false".to_string())),
            ReportUploadMode::Misconfigured
        ));
        assert!(matches!(
            ReportUploadMode::resolve(None, Some("".to_string())),
            ReportUploadMode::Misconfigured
        ));
    }

    #[test]
    fn report_mode_local_dev_only_on_explicit_optin() {
        for v in ["true", "1", "yes", "on", "TRUE", " On "] {
            assert!(
                matches!(
                    ReportUploadMode::resolve(None, Some(v.to_string())),
                    ReportUploadMode::LocalDev
                ),
                "flag value {v:?} should enable LocalDev"
            );
        }
    }

    #[test]
    fn env_flag_truthiness() {
        assert!(env_flag_enabled(Some("true")));
        assert!(env_flag_enabled(Some("1")));
        assert!(env_flag_enabled(Some(" YES ")));
        assert!(!env_flag_enabled(Some("false")));
        assert!(!env_flag_enabled(Some("0")));
        assert!(!env_flag_enabled(Some("")));
        assert!(!env_flag_enabled(None));
    }
}
