use crate::dates::{civil_from_days, days_from_civil};
use crate::Result;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
struct ResolvedCreds {
    access_key: String,
    secret_key: String,
    session_token: Option<String>,
}

struct CachedCreds {
    creds: ResolvedCreds,
    expires_epoch: u64,
}

enum CredsSource {
    Static(ResolvedCreds),
    Container {
        url: String,
        auth_token: Option<String>,
        cache: Mutex<Option<CachedCreds>>,
    },
}

pub struct Space {
    pub scheme: String,

    pub host: String,

    pub region: String,

    pub bucket: Option<String>,

    pub path_style: bool,
    pub read_only: bool,
    creds: CredsSource,
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(60)))
            .build()
            .into()
    })
}

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

fn uri_encode_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for b in key.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex(&h.finalize())
}
fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut m = HmacSha256::new_from_slice(key).expect("hmac key");
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn timestamps() -> (String, String) {
    let secs = now_epoch();
    let days = (secs / 86_400) as i64;
    let sod = (secs % 86_400) as i64;
    let (h, mi, s) = (sod / 3600, (sod % 3600) / 60, sod % 60);
    let (y, mo, d) = civil_from_days(days);
    let date = format!("{y:04}{mo:02}{d:02}");
    (date.clone(), format!("{date}T{h:02}{mi:02}{s:02}Z"))
}

fn parse_iso8601_epoch(s: &str) -> Option<u64> {
    if s.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r).and_then(|t| t.parse::<i64>().ok());
    let y = num(0..4)?;
    let mo = num(5..7)?;
    let d = num(8..10)?;
    let h = num(11..13)?;
    let mi = num(14..16)?;
    let sec = num(17..19)?;
    let t = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec;
    u64::try_from(t).ok()
}

fn warn_once_403(key: &str) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(key = %key, "space GET 403, treating as miss; if unexpected check credentials and bucket policy (warned once)");
    }
}

impl Space {
    pub fn from_env() -> Option<Space> {
        let first = |vars: &[&str]| {
            vars.iter()
                .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        };
        let static_creds = match (
            first(&["ABGEN_S3_ACCESS_KEY", "AWS_ACCESS_KEY_ID"]),
            first(&["ABGEN_S3_SECRET_KEY", "AWS_SECRET_ACCESS_KEY"]),
        ) {
            (Some(access_key), Some(secret_key)) => Some(ResolvedCreds {
                access_key,
                secret_key,
                session_token: first(&["ABGEN_S3_SESSION_TOKEN", "AWS_SESSION_TOKEN"]),
            }),
            _ => None,
        };
        let creds = match static_creds {
            Some(c) => CredsSource::Static(c),
            None => {
                let url = first(&["AWS_CONTAINER_CREDENTIALS_FULL_URI"]).or_else(|| {
                    first(&["AWS_CONTAINER_CREDENTIALS_RELATIVE_URI"])
                        .map(|u| format!("http://169.254.170.2{u}"))
                })?;
                CredsSource::Container {
                    url,
                    auth_token: first(&["AWS_CONTAINER_AUTHORIZATION_TOKEN"]),
                    cache: Mutex::new(None),
                }
            }
        };

        let endpoint = first(&["ABGEN_S3_ENDPOINT"])?;
        let (scheme, host) = match endpoint.split_once("://") {
            Some((s, h)) => (s.to_string(), h.trim_end_matches('/').to_string()),
            None => (
                "https".to_string(),
                endpoint.trim_end_matches('/').to_string(),
            ),
        };

        let region =
            first(&["ABGEN_S3_REGION", "AWS_REGION"]).unwrap_or_else(|| "us-east-1".to_string());
        let bucket = std::env::var("ABGEN_S3_BUCKET")
            .ok()
            .filter(|s| !s.is_empty());
        let path_style = crate::clihelp::env_bool("ABGEN_S3_PATH_STYLE", false);
        let read_only = crate::clihelp::env_bool("ABGEN_S3_READ_ONLY", false);

        Some(Space {
            scheme,
            host,
            region,
            bucket,
            path_style,
            read_only,
            creds,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_static_creds(
        scheme: &str,
        host: &str,
        region: &str,
        bucket: Option<&str>,
        path_style: bool,
        read_only: bool,
        access_key: &str,
        secret_key: &str,
    ) -> Space {
        Space {
            scheme: scheme.to_string(),
            host: host.to_string(),
            region: region.to_string(),
            bucket: bucket.map(str::to_string),
            path_style,
            read_only,
            creds: CredsSource::Static(ResolvedCreds {
                access_key: access_key.to_string(),
                secret_key: secret_key.to_string(),
                session_token: None,
            }),
        }
    }

    pub fn creds_source(&self) -> &'static str {
        match &self.creds {
            CredsSource::Static(c) if c.session_token.is_some() => "static-env+session-token",
            CredsSource::Static(_) => "static-env",
            CredsSource::Container { .. } => "ecs-container-role",
        }
    }

    fn creds(&self) -> Result<ResolvedCreds> {
        match &self.creds {
            CredsSource::Static(c) => Ok(c.clone()),
            CredsSource::Container {
                url,
                auth_token,
                cache,
            } => {
                let now = now_epoch();
                {
                    let guard = cache.lock().unwrap_or_else(|p| p.into_inner());
                    if let Some(c) = guard.as_ref() {
                        if now + 300 < c.expires_epoch {
                            return Ok(c.creds.clone());
                        }
                    }
                }
                let mut req = agent().get(url);
                if let Some(t) = auth_token {
                    req = req.header("Authorization", t);
                }
                let resp = req
                    .call()
                    .map_err(|e| crate::anyhow!("container credentials GET: {e}"))?;
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut resp.into_body().into_reader(), &mut buf)?;
                let v: serde_json::Value = serde_json::from_slice(&buf)
                    .map_err(|e| crate::anyhow!("container credentials parse: {e}"))?;
                let field = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
                let creds = ResolvedCreds {
                    access_key: field("AccessKeyId")
                        .ok_or_else(|| crate::anyhow!("container credentials: no AccessKeyId"))?,
                    secret_key: field("SecretAccessKey").ok_or_else(|| {
                        crate::anyhow!("container credentials: no SecretAccessKey")
                    })?,
                    session_token: field("Token"),
                };
                let expires_epoch = field("Expiration")
                    .and_then(|e| parse_iso8601_epoch(&e))
                    .unwrap_or(now + 900);
                *cache.lock().unwrap_or_else(|p| p.into_inner()) = Some(CachedCreds {
                    creds: creds.clone(),
                    expires_epoch,
                });
                Ok(creds)
            }
        }
    }

    fn path(&self, key: &str) -> String {
        let encoded = uri_encode_key(key);
        match (self.path_style, &self.bucket) {
            (true, Some(b)) => format!("/{b}/{encoded}"),
            _ => format!("/{encoded}"),
        }
    }

    fn authorize(
        &self,
        c: &ResolvedCreds,
        method: &str,
        key: &str,
        payload_hash: &str,
        amz_date: &str,
        date: &str,
    ) -> String {
        let canonical_uri = self.path(key);
        let (signed_headers, canonical_headers) = match &c.session_token {
            Some(token) => (
                "host;x-amz-content-sha256;x-amz-date;x-amz-security-token",
                format!(
                    "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\nx-amz-security-token:{}\n",
                    self.host, payload_hash, amz_date, token
                ),
            ),
            None => (
                "host;x-amz-content-sha256;x-amz-date",
                format!(
                    "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
                    self.host, payload_hash, amz_date
                ),
            ),
        };
        let canonical_request = format!(
            "{method}\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
        );
        let scope = format!("{date}/{}/s3/aws4_request", self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            sha256_hex(canonical_request.as_bytes())
        );
        let k_date = hmac(format!("AWS4{}", c.secret_key).as_bytes(), date.as_bytes());
        let k_region = hmac(&k_date, self.region.as_bytes());
        let k_service = hmac(&k_region, b"s3");
        let k_signing = hmac(&k_service, b"aws4_request");
        let signature = hex(&hmac(&k_signing, string_to_sign.as_bytes()));
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            c.access_key
        )
    }

    fn call_get(
        &self,
        key: &str,
    ) -> Result<std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>> {
        let c = self.creds()?;
        let payload_hash = sha256_hex(b"");
        let (date, amz) = timestamps();
        let auth = self.authorize(&c, "GET", key, &payload_hash, &amz, &date);
        let url = format!("{}://{}{}", self.scheme, self.host, self.path(key));
        let mut req = agent()
            .get(&url)
            .header("x-amz-date", &amz)
            .header("x-amz-content-sha256", &payload_hash)
            .header("Authorization", &auth);
        if let Some(token) = &c.session_token {
            req = req.header("x-amz-security-token", token);
        }
        Ok(req.call())
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match self.call_get(key)? {
            Ok(r) => {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut r.into_body().into_reader(), &mut buf)?;
                Ok(Some(buf))
            }
            Err(ureq::Error::StatusCode(404)) => Ok(None),
            Err(ureq::Error::StatusCode(403)) => {
                warn_once_403(key);
                Ok(None)
            }
            Err(e) => Err(crate::anyhow!("space GET {key}: {e}")),
        }
    }

    pub fn get_status(&self, key: &str) -> Result<u16> {
        match self.call_get(key)? {
            Ok(r) => Ok(r.status().as_u16()),
            Err(ureq::Error::StatusCode(code)) => Ok(code),
            Err(e) => Err(crate::anyhow!("space GET {key}: {e}")),
        }
    }

    pub fn put(&self, key: &str, body: &[u8], content_type: &str) -> Result<()> {
        if self.read_only {
            return Err(crate::anyhow!(
                "space is read-only (ABGEN_S3_READ_ONLY): refusing PUT {key}"
            ));
        }
        let c = self.creds()?;
        let payload_hash = sha256_hex(body);
        let (date, amz) = timestamps();
        let auth = self.authorize(&c, "PUT", key, &payload_hash, &amz, &date);
        let url = format!("{}://{}{}", self.scheme, self.host, self.path(key));
        let mut req = agent()
            .put(&url)
            .header("x-amz-date", &amz)
            .header("x-amz-content-sha256", &payload_hash)
            .header("Authorization", &auth)
            .header("Content-Type", content_type);
        if let Some(token) = &c.session_token {
            req = req.header("x-amz-security-token", token);
        }
        req.send(body)
            .map_err(|e| crate::anyhow!("space PUT {key}: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_space(read_only: bool, session_token: Option<&str>) -> Space {
        Space {
            scheme: "https".to_string(),
            host: "bucket.example.com".to_string(),
            region: "us-east-1".to_string(),
            bucket: Some("bucket".to_string()),
            path_style: false,
            read_only,
            creds: CredsSource::Static(ResolvedCreds {
                access_key: "AKIATEST".to_string(),
                secret_key: "secret".to_string(),
                session_token: session_token.map(str::to_string),
            }),
        }
    }

    #[test]
    fn session_token_changes_signed_headers() {
        let with = test_space(false, Some("tok123"));
        let without = test_space(false, None);
        let hash = sha256_hex(b"");
        let cw = match &with.creds {
            CredsSource::Static(c) => c.clone(),
            _ => unreachable!(),
        };
        let co = match &without.creds {
            CredsSource::Static(c) => c.clone(),
            _ => unreachable!(),
        };
        let a1 = with.authorize(&cw, "GET", "k", &hash, "20260101T000000Z", "20260101");
        let a2 = without.authorize(&co, "GET", "k", &hash, "20260101T000000Z", "20260101");
        assert!(a1.contains("x-amz-security-token"));
        assert!(!a2.contains("x-amz-security-token"));
        assert_ne!(a1, a2);
    }

    #[test]
    fn read_only_put_refuses_without_network() {
        let s = test_space(true, None);
        let err = s.put("k", b"x", "text/plain").unwrap_err();
        assert!(err.to_string().contains("read-only"));
    }

    fn capture_one_request(
        listener: std::net::TcpListener,
        out: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) -> std::thread::JoinHandle<()> {
        use std::io::{BufRead, BufReader, Read, Write};
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head: Vec<String> = Vec::new();
            let mut content_len = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    break;
                }
                let t = line.trim_end().to_string();
                if t.is_empty() {
                    break;
                }
                if let Some(v) = t.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_len = v.trim().parse().unwrap_or(0);
                }
                head.push(t);
            }
            if content_len > 0 {
                let mut body = vec![0u8; content_len];
                let _ = reader.read_exact(&mut body);
            }
            out.lock().unwrap().push(head.join("\n"));
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = stream.flush();
        })
    }

    #[test]
    fn put_signs_every_amz_header_it_sends() {
        for token in [None, Some("tok123")] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let handle = capture_one_request(listener, captured.clone());
            let mut s = test_space(false, token);
            s.scheme = "http".to_string();
            s.host = addr.to_string();
            s.put(
                "v41/cid/Qmhash_windows",
                b"body",
                "application/octet-stream",
            )
            .unwrap();
            handle.join().unwrap();
            let head = captured.lock().unwrap().join("\n");
            let amz_sent: Vec<String> = head
                .lines()
                .filter_map(|l| {
                    let (name, _) = l.split_once(':')?;
                    let n = name.trim().to_ascii_lowercase();
                    n.starts_with("x-amz-").then_some(n)
                })
                .collect();
            assert!(!amz_sent.is_empty(), "{head}");
            assert!(!amz_sent.contains(&"x-amz-acl".to_string()), "{head}");
            let auth = head
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                .unwrap();
            let signed: Vec<&str> = auth
                .split("SignedHeaders=")
                .nth(1)
                .unwrap()
                .split(',')
                .next()
                .unwrap()
                .split(';')
                .collect();
            for h in &amz_sent {
                assert!(signed.contains(&h.as_str()), "unsigned {h} in {auth}");
            }
        }
    }

    #[test]
    fn uri_encoding_noop_for_existing_key_shapes() {
        for key in [
            "manifest/bafkEntity_windows.json",
            "v41/bafkScene/Qmhash_mac.br",
            "LOD/1/bafkscene_1_windows",
            "lods-unity/manifests/bafkscene_InitialSceneState.json",
            "v41/dcl/scene_ignore_windows",
        ] {
            assert_eq!(uri_encode_key(key), key);
        }
        let s = test_space(false, None);
        assert_eq!(
            s.path("v41/bafkScene/Qmhash_mac"),
            "/v41/bafkScene/Qmhash_mac"
        );
    }

    #[test]
    fn uri_encoding_escapes_spaces_in_canonical_and_url() {
        let key = "v41/dcl/universal render pipeline/lit_ignore_windows";
        assert_eq!(
            uri_encode_key(key),
            "v41/dcl/universal%20render%20pipeline/lit_ignore_windows"
        );
        let s = test_space(false, None);
        assert_eq!(
            s.path(key),
            "/v41/dcl/universal%20render%20pipeline/lit_ignore_windows"
        );
        let ps = Space::with_static_creds(
            "https",
            "s3.example.com",
            "us-east-1",
            Some("bkt"),
            true,
            false,
            "AKIATEST",
            "secret",
        );
        assert_eq!(
            ps.path(key),
            "/bkt/v41/dcl/universal%20render%20pipeline/lit_ignore_windows"
        );
        let c = match &ps.creds {
            CredsSource::Static(c) => c.clone(),
            _ => unreachable!(),
        };
        let hash = sha256_hex(b"");
        let a1 = ps.authorize(&c, "GET", key, &hash, "20260101T000000Z", "20260101");
        let a2 = ps.authorize(
            &c,
            "GET",
            "v41/dcl/other shader",
            &hash,
            "20260101T000000Z",
            "20260101",
        );
        assert_ne!(a1, a2);
    }

    #[test]
    fn iso8601_epoch_roundtrips_with_civil_days() {
        assert_eq!(parse_iso8601_epoch("1970-01-01T00:00:00Z"), Some(0));
        for days in [0i64, 19_723, 20_644, 25_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        let e = parse_iso8601_epoch("2026-07-10T01:02:03Z").unwrap();
        assert_eq!(e % 86_400, 3600 + 2 * 60 + 3);
        let (y, m, d) = civil_from_days((e / 86_400) as i64);
        assert_eq!((y, m, d), (2026, 7, 10));
    }
}
