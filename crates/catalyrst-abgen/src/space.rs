
use crate::Result;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

pub struct Space {

    pub scheme: String,

    pub host: String,

    pub region: String,

    pub bucket: Option<String>,

    pub path_style: bool,
    pub access_key: String,
    pub secret_key: String,
}

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
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

fn timestamps() -> (String, String) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let sod = (secs % 86_400) as i64;
    let (h, mi, s) = (sod / 3600, (sod % 3600) / 60, sod % 60);
    let (y, mo, d) = civil_from_days(days);
    let date = format!("{y:04}{mo:02}{d:02}");
    (date.clone(), format!("{date}T{h:02}{mi:02}{s:02}Z"))
}

const fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

impl Space {

    pub fn from_env() -> Option<Space> {
        let first = |vars: &[&str]| {
            vars.iter()
                .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        };
        let access_key =
            first(&["ABGEN_S3_ACCESS_KEY", "AWS_ACCESS_KEY_ID", "DO_SPACE_ACCESS_KEY"])?;
        let secret_key =
            first(&["ABGEN_S3_SECRET_KEY", "AWS_SECRET_ACCESS_KEY", "DO_SPACE_SECRET_KEY"])?;

        let endpoint = first(&["ABGEN_S3_ENDPOINT", "ABGEN_SPACE_HOST"])
            .unwrap_or_else(|| "ab-cdn.ams3.digitaloceanspaces.com".to_string());
        let (scheme, host) = match endpoint.split_once("://") {
            Some((s, h)) => (s.to_string(), h.trim_end_matches('/').to_string()),
            None => ("https".to_string(), endpoint.trim_end_matches('/').to_string()),
        };

        let region =
            first(&["ABGEN_S3_REGION", "AWS_REGION", "ABGEN_SPACE_REGION"]).unwrap_or_else(|| "ams3".to_string());
        let bucket = std::env::var("ABGEN_S3_BUCKET").ok().filter(|s| !s.is_empty());
        let path_style = matches!(
            std::env::var("ABGEN_S3_PATH_STYLE").ok().as_deref(),
            Some("1" | "true" | "yes")
        );

        Some(Space { scheme, host, region, bucket, path_style, access_key, secret_key })
    }

    fn path(&self, key: &str) -> String {
        match (self.path_style, &self.bucket) {
            (true, Some(b)) => format!("/{b}/{key}"),
            _ => format!("/{key}"),
        }
    }

    fn authorize(&self, method: &str, key: &str, payload_hash: &str, amz_date: &str, date: &str) -> String {
        let canonical_uri = self.path(key);
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_headers = format!(
            "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
            self.host, payload_hash, amz_date
        );
        let canonical_request = format!(
            "{method}\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
        );
        let scope = format!("{date}/{}/s3/aws4_request", self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            sha256_hex(canonical_request.as_bytes())
        );
        let k_date = hmac(format!("AWS4{}", self.secret_key).as_bytes(), date.as_bytes());
        let k_region = hmac(&k_date, self.region.as_bytes());
        let k_service = hmac(&k_region, b"s3");
        let k_signing = hmac(&k_service, b"aws4_request");
        let signature = hex(&hmac(&k_signing, string_to_sign.as_bytes()));
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.access_key
        )
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let payload_hash = sha256_hex(b"");
        let (date, amz) = timestamps();
        let auth = self.authorize("GET", key, &payload_hash, &amz, &date);
        let url = format!("{}://{}{}", self.scheme, self.host, self.path(key));
        let resp = ureq::get(&url)
            .header("x-amz-date", &amz)
            .header("x-amz-content-sha256", &payload_hash)
            .header("Authorization", &auth)
            .call();
        match resp {
            Ok(r) => {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut r.into_body().into_reader(), &mut buf)?;
                Ok(Some(buf))
            }
            Err(ureq::Error::StatusCode(404)) | Err(ureq::Error::StatusCode(403)) => Ok(None),
            Err(e) => Err(crate::anyhow!("space GET {key}: {e}")),
        }
    }

    pub fn put(&self, key: &str, body: &[u8], content_type: &str) -> Result<()> {
        let payload_hash = sha256_hex(body);
        let (date, amz) = timestamps();
        let auth = self.authorize("PUT", key, &payload_hash, &amz, &date);
        let url = format!("{}://{}{}", self.scheme, self.host, self.path(key));
        ureq::put(&url)
            .header("x-amz-date", &amz)
            .header("x-amz-content-sha256", &payload_hash)
            .header("Authorization", &auth)
            .header("Content-Type", content_type)
            .header("x-amz-acl", "private")
            .send(body)
            .map_err(|e| crate::anyhow!("space PUT {key}: {e}"))?;
        Ok(())
    }
}
