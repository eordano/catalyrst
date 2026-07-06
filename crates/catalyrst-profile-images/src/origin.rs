use std::time::Duration;

use bytes::Bytes;

use crate::cache::ImageKind;

const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

pub enum OriginResult {
    Hit(Bytes),

    NotFound,

    Error(String),
}

pub struct Origin {
    client: reqwest::Client,
    base_url: String,
}

impl Origin {
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent("catalyrst-profile-images/0.1")
            .build()
            .expect("reqwest client");
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn fetch(&self, entity: &str, kind: ImageKind) -> OriginResult {
        let url = format!("{}/entities/{}/{}", self.base_url, entity, kind.filename());
        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return OriginResult::Error(format!("request failed: {e}")),
        };
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return OriginResult::NotFound;
        }
        if !resp.status().is_success() {
            return OriginResult::Error(format!("origin status {}", resp.status()));
        }
        if let Some(len) = resp.content_length() {
            if len as usize > MAX_IMAGE_BYTES {
                return OriginResult::Error(format!("image too large: {len} bytes"));
            }
        }
        match resp.bytes().await {
            Ok(b) if b.len() <= MAX_IMAGE_BYTES => OriginResult::Hit(b),
            Ok(_) => OriginResult::Error("image too large".to_string()),
            Err(e) => OriginResult::Error(format!("read failed: {e}")),
        }
    }
}
