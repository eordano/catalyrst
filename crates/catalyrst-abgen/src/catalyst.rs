use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;
use std::time::Duration;

pub const DEFAULT_CATALYST: &str = "http://localhost:5141/content";
pub const UA: &str = "ab-generator/1.0 (+https://catalyst.dcl.one)";
const HTTP_TIMEOUT_SECS: u64 = 60;
const HTTP_RETRIES: u32 = 3;

#[derive(Clone, Debug)]
pub struct ContentEntry {
    pub file: String,
    pub hash: String,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub entity_id: String,
    pub entity_type: String,
    pub pointers: Vec<String>,
    pub content: Vec<ContentEntry>,
    pub metadata: serde_json::Value,
}

impl Scene {
    pub fn content_by_file(&self) -> HashMap<String, String> {
        self.content
            .iter()
            .map(|c| (c.file.to_lowercase(), c.hash.clone()))
            .collect()
    }

    pub fn files_with_ext(&self, exts: &[&str]) -> Vec<ContentEntry> {
        let lowered: Vec<String> = exts.iter().map(|e| e.to_lowercase()).collect();
        self.content
            .iter()
            .filter(|c| {
                let f = c.file.to_lowercase();
                lowered.iter().any(|e| f.ends_with(e.as_str()))
            })
            .cloned()
            .collect()
    }
}

fn is_pointer(target: &str) -> bool {
    let t = target.trim();
    let parts: Vec<&str> = t.split(',').collect();
    if parts.len() != 2 {
        return false;
    }
    parse_int_token(parts[0]) && parse_int_token(parts[1])
}

fn parse_int_token(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let digits = if bytes[0] == b'-' { &bytes[1..] } else { bytes };
    !digits.is_empty() && digits.iter().all(|b| b.is_ascii_digit())
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_secs_f64(0.5 * 2f64.powi(attempt as i32))
}

pub struct CatalystClient {
    base: String,
    agent: ureq::Agent,
    local: Option<crate::local_store::LocalContentStore>,
}

impl CatalystClient {
    pub fn new(base_url: &str) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(HTTP_TIMEOUT_SECS)))
            .build()
            .into();
        CatalystClient {
            base: base_url.trim_end_matches('/').to_string(),
            agent,
            local: None,
        }
    }

    fn with_local_store(mut self, store: crate::local_store::LocalContentStore) -> Self {
        self.local = Some(store);
        self
    }

    pub fn from_args(catalyst: &str, local_root: Option<&str>) -> Self {
        let cli = Self::new(catalyst);
        match local_root {
            None => cli,
            Some(p) => {
                let root = if p.is_empty() {
                    crate::local_store::DEFAULT_CONTENT_ROOT
                } else {
                    p
                };
                eprintln!("--local: content fetches served from {root}");
                cli.with_local_store(crate::local_store::LocalContentStore::new(root))
            }
        }
    }

    fn get(&self, path: &str) -> Result<Vec<u8>> {
        let url = format!("{}{}", self.base, path);
        let mut last: Option<String> = None;
        for attempt in 0..HTTP_RETRIES {
            match self.agent.get(&url).header("User-Agent", UA).call() {
                Ok(resp) => {
                    let mut buf: Vec<u8> = Vec::new();
                    resp.into_body().into_reader().read_to_end(&mut buf)?;
                    return Ok(buf);
                }
                Err(ureq::Error::StatusCode(code)) => {
                    if code == 404 {
                        bail!("404 {}", url);
                    }
                    last = Some(format!("HTTP {code}"));
                }
                Err(e) => {
                    last = Some(e.to_string());
                }
            }
            std::thread::sleep(backoff(attempt));
        }
        bail!("GET {} failed: {}", url, last.unwrap_or_default())
    }

    fn post_json(&self, path: &str, body: &serde_json::Value) -> Result<Vec<u8>> {
        let url = format!("{}{}", self.base, path);
        let mut last: Option<String> = None;
        for attempt in 0..HTTP_RETRIES {
            match self
                .agent
                .post(&url)
                .header("User-Agent", UA)
                .header("Content-Type", "application/json")
                .send(body.to_string())
            {
                Ok(resp) => {
                    let mut buf: Vec<u8> = Vec::new();
                    resp.into_body().into_reader().read_to_end(&mut buf)?;
                    return Ok(buf);
                }
                Err(ureq::Error::StatusCode(code)) => {
                    last = Some(format!("HTTP {code}"));
                }
                Err(e) => {
                    last = Some(e.to_string());
                }
            }
            std::thread::sleep(backoff(attempt));
        }
        bail!("POST {} failed: {}", url, last.unwrap_or_default())
    }

    pub fn fetch_content(&self, content_hash: &str) -> Result<Vec<u8>> {
        if let Some(store) = &self.local {
            match store.fetch(content_hash) {
                Ok(b) => return Ok(b),
                Err(e) => {
                    return Err(anyhow!(
                        "local content store has no CID {content_hash}: {e}"
                    ));
                }
            }
        }
        self.get(&format!("/contents/{content_hash}"))
    }

    /// Fetch an entity by its CID and parse it into a `Scene`. The entity file is
    /// itself content-addressed, so it lives in the content store / `/contents`
    /// under the entity id — works disk-or-remote via `fetch_content`. Used by the
    /// unified AB serving for native content passthrough (entity -> file -> hash).
    pub fn fetch_entity(&self, entity_id: &str) -> Result<Scene> {
        let raw = self.fetch_content(entity_id)?;
        let mut v: serde_json::Value = serde_json::from_slice(&raw)
            .map_err(|e| anyhow!("entity {entity_id} is not JSON: {e}"))?;
        // The stored entity file is content-addressed and omits "id" (it IS the
        // CID). `parse_entity` requires it, so inject the known id when absent —
        // works for both the raw on-disk file and the /contents HTTP response.
        if v.get("id").and_then(|x| x.as_str()).is_none() {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("id".to_string(), serde_json::Value::String(entity_id.to_string()));
            }
        }
        Self::parse_entity(&v)
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    fn parse_entity(ent: &serde_json::Value) -> Result<Scene> {
        let entity_id = ent
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("entity has no id"))?
            .to_string();
        let entity_type = ent
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let pointers = ent
            .get("pointers")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let content = ent
            .get("content")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|c| {
                        let file = c.get("file")?.as_str()?.to_string();
                        let hash = c.get("hash")?.as_str()?.to_string();
                        Some(ContentEntry { file, hash })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let metadata = ent
            .get("metadata")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        Ok(Scene {
            entity_id,
            entity_type,
            pointers,
            content,
            metadata,
        })
    }

    pub fn resolve_entities(&self, pointers: &[String]) -> Result<Vec<Scene>> {
        if pointers.is_empty() {
            return Ok(Vec::new());
        }
        let body = serde_json::json!({ "pointers": pointers });
        let raw = self.post_json("/entities/active", &body)?;
        let arr: serde_json::Value = serde_json::from_slice(&raw)?;
        let arr = arr
            .as_array()
            .ok_or_else(|| anyhow!("entities/active did not return an array"))?;
        arr.iter().map(Self::parse_entity).collect()
    }

    pub fn collection_member_urns(
        &self,
        lambdas_base: &str,
        collection_id: &str,
    ) -> Result<Vec<String>> {
        let lambdas = lambdas_base.trim_end_matches('/');
        let url = format!("{lambdas}/collections/wearables?collectionId={collection_id}");
        let mut last: Option<String> = None;
        let raw: Option<Vec<u8>> = (|| {
            for attempt in 0..HTTP_RETRIES {
                match self.agent.get(&url).header("User-Agent", UA).call() {
                    Ok(resp) => {
                        let mut buf: Vec<u8> = Vec::new();
                        if let Err(e) = resp.into_body().into_reader().read_to_end(&mut buf) {
                            last = Some(e.to_string());
                        } else {
                            return Some(buf);
                        }
                    }
                    Err(ureq::Error::StatusCode(code)) => {
                        last = Some(format!("HTTP {code}"));
                        if code == 404 {
                            return None;
                        }
                    }
                    Err(e) => last = Some(e.to_string()),
                }
                std::thread::sleep(backoff(attempt));
            }
            None
        })();
        let raw = raw.ok_or_else(|| anyhow!("GET {} failed: {}", url, last.unwrap_or_default()))?;
        let doc: serde_json::Value = serde_json::from_slice(&raw)?;
        let wearables = doc
            .get("wearables")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("collection response had no 'wearables' array"))?;
        let urns: Vec<String> = wearables
            .iter()
            .filter_map(|w| w.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect();
        Ok(urns)
    }

    pub fn resolve_scene(&self, target: &str) -> Result<Scene> {
        let ent: serde_json::Value = if is_pointer(target) {
            let body = serde_json::json!({ "pointers": [target.trim()] });
            let raw = self.post_json("/entities/active", &body)?;
            let arr: serde_json::Value = serde_json::from_slice(&raw)?;
            let arr = arr
                .as_array()
                .ok_or_else(|| anyhow!("entities/active did not return an array"))?;
            if arr.is_empty() {
                bail!("no active entity at pointer {:?}", target);
            }
            arr[0].clone()
        } else {
            let raw = self.fetch_content(target)?;
            let mut ent: serde_json::Value = serde_json::from_slice(&raw)?;
            if ent.get("id").is_none() {
                if let Some(obj) = ent.as_object_mut() {
                    obj.insert(
                        "id".to_string(),
                        serde_json::Value::String(target.to_string()),
                    );
                }
            }
            ent
        };

        Self::parse_entity(&ent)
    }
}

use std::io::Read;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_detection() {
        assert!(is_pointer("-9,-9"));
        assert!(is_pointer("0,0"));
        assert!(is_pointer(" 12,-34 "));
        assert!(is_pointer("100,200"));
        assert!(!is_pointer("abc"));
        assert!(!is_pointer("1,2,3"));
        assert!(!is_pointer("1.5,2"));
        assert!(!is_pointer("Qm..."));
        assert!(!is_pointer(",5"));
    }

    #[test]
    fn content_helpers() {
        let s = Scene {
            entity_id: "e".into(),
            entity_type: String::new(),
            pointers: vec![],
            content: vec![
                ContentEntry {
                    file: "Models/A.GLB".into(),
                    hash: "h1".into(),
                },
                ContentEntry {
                    file: "tex/b.png".into(),
                    hash: "h2".into(),
                },
            ],
            metadata: serde_json::json!({}),
        };
        let map = s.content_by_file();
        assert_eq!(map.get("models/a.glb").map(String::as_str), Some("h1"));
        let glbs = s.files_with_ext(&[".glb"]);
        assert_eq!(glbs.len(), 1);
        assert_eq!(glbs[0].hash, "h1");
    }
}
