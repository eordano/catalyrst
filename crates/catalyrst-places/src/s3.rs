use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone)]
pub struct S3Config {
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,

    pub bucket_hostname: Option<String>,
    pub region: String,

    pub endpoint: Option<String>,
}

impl S3Config {
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

#[derive(Debug, Clone)]
pub enum ReportUploadMode {
    S3(S3Config),

    LocalDev,

    Misconfigured,
}

pub const REPORT_LOCAL_FALLBACK_ENV: &str = "PLACES_REPORT_LOCAL_FALLBACK";

impl ReportUploadMode {
    pub fn from_env() -> Self {
        Self::resolve(
            S3Config::from_env(),
            std::env::var(REPORT_LOCAL_FALLBACK_ENV).ok(),
        )
    }

    pub fn resolve(s3: Option<S3Config>, fallback_flag: Option<String>) -> Self {
        match s3 {
            Some(cfg) => Self::S3(cfg),
            None if env_flag_enabled(fallback_flag.as_deref()) => Self::LocalDev,
            None => Self::Misconfigured,
        }
    }
}

fn env_flag_enabled(v: Option<&str>) -> bool {
    matches!(
        v.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("true") | Some("1") | Some("yes") | Some("on")
    )
}

pub struct PresignPutObject<'a> {
    pub key: &'a str,

    pub content_type: &'a str,

    pub acl: &'a str,

    pub cache_control: &'a str,

    pub expires: u64,

    pub metadata: &'a [(String, String)],
}

pub fn presign_put_object(
    cfg: &S3Config,
    req: &PresignPutObject<'_>,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
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

    let expires_abs = now.timestamp() + req.expires as i64;

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

    let canonical_resource = match &cfg.endpoint {
        Some(_) => url_path.clone(),
        None => format!("/{}{}", cfg.bucket, url_path),
    };

    let string_to_sign = format!(
        "PUT\n\n{}\n{}\n{}\n{}",
        req.content_type, expires_abs, canonical_amz_headers, canonical_resource
    );

    let signature = sign_sha1_base64(&cfg.secret_key, string_to_sign.as_bytes());

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

        assert!(url.ends_with("Signature=Br7FhFIMDcrqVxHZYsLZn6OlUC8%3D&x-amz-acl=private&x-amz-meta-address=0xabc&x-amz-meta-signer=dcl%3Aexplorer"));
    }

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
