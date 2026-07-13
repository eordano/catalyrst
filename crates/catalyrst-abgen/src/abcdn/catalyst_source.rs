use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;

use dcl_contents::errors::ApiError;
use dcl_contents::registry::{async_trait, EntitySource};
use dcl_contents::types::{ActiveEntity, ContentFile};

const PROXY_TIMEOUT_SECS: u64 = 10;

#[derive(Clone)]
pub struct CatalystEntitySource {
    base: String,
    worlds_url: Option<String>,
    agent: ureq::Agent,
}

impl CatalystEntitySource {
    pub fn new(base: &str, worlds_url: Option<String>) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(PROXY_TIMEOUT_SECS)))
            .build()
            .into();
        Self {
            base: base.trim_end_matches('/').to_string(),
            worlds_url,
            agent,
        }
    }

    fn post_active(&self, body: serde_json::Value) -> Result<Vec<ActiveEntity>, ApiError> {
        let url = format!("{}/entities/active", self.base);
        let resp = self
            .agent
            .post(&url)
            .header("User-Agent", crate::catalyst::UA)
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .map_err(|e| upstream(&url, e))?;
        let mut buf: Vec<u8> = Vec::new();
        resp.into_body()
            .into_reader()
            .read_to_end(&mut buf)
            .map_err(|e| upstream(&url, e))?;
        let parsed: serde_json::Value =
            serde_json::from_slice(&buf).map_err(|e| upstream(&url, e))?;
        let arr = parsed
            .as_array()
            .ok_or_else(|| upstream(&url, "response is not an array"))?;
        Ok(arr.iter().filter_map(parse_active_entity).collect())
    }

    fn fetch_union(&self, queries: &[String]) -> Result<Vec<ActiveEntity>, ApiError> {
        let lowered: Vec<String> = queries.iter().map(|q| q.to_lowercase()).collect();
        let by_pointer = self.post_active(serde_json::json!({ "pointers": lowered }))?;
        let by_id = self.post_active(serde_json::json!({ "ids": queries }))?;

        let mut by_entity: HashMap<String, ActiveEntity> = HashMap::new();
        for ent in by_pointer.into_iter().chain(by_id) {
            match by_entity.entry(ent.entity_id.clone()) {
                Entry::Occupied(mut cur) => {
                    if ent.timestamp > cur.get().timestamp {
                        cur.insert(ent);
                    }
                }
                Entry::Vacant(slot) => {
                    slot.insert(ent);
                }
            }
        }
        Ok(by_entity.into_values().collect())
    }
}

#[async_trait]
impl EntitySource for CatalystEntitySource {
    async fn resolve_pointers(&self, pointers: &[String]) -> Result<Vec<ActiveEntity>, ApiError> {
        if pointers.is_empty() {
            return Ok(Vec::new());
        }
        let src = self.clone();
        let queries = pointers.to_vec();
        run_blocking(move || src.fetch_union(&queries)).await
    }

    async fn resolve_profiles(&self, addresses: &[String]) -> Result<Vec<ActiveEntity>, ApiError> {
        if addresses.is_empty() {
            return Ok(Vec::new());
        }
        let src = self.clone();
        let lowered: Vec<String> = addresses.iter().map(|a| a.to_lowercase()).collect();
        run_blocking(move || {
            let mut ents = src.post_active(serde_json::json!({ "pointers": lowered }))?;
            ents.retain(|e| e.entity_type == "profile");
            Ok(ents)
        })
        .await
    }

    async fn resolve_world(&self, world_name: &str) -> Result<Vec<ActiveEntity>, ApiError> {
        let Some(worlds_url) = self.worlds_url.clone() else {
            return Ok(Vec::new());
        };
        let name = world_name.to_string();
        run_blocking(move || {
            let secs = crate::worlds::SERVE_FETCH_TIMEOUT_SECS;
            let scenes = match crate::worlds::resolve_world_bounded(&worlds_url, &name, secs) {
                Ok(scenes) => scenes,
                Err(e) if world_not_found(&e) => return Ok(Vec::new()),
                Err(e) => return Err(ApiError::upstream(format!("world {name}: {e:#}"))),
            };
            let mut out = Vec::new();
            for scene in scenes {
                match crate::worlds::fetch_scene_entity(&scene, secs) {
                    Ok(v) => out.extend(parse_active_entity(&v)),
                    Err(e) => tracing::warn!(
                        entity = %scene.entity_id,
                        error = %format!("{e:#}"),
                        "registry proxy: world scene entity fetch failed"
                    ),
                }
            }
            Ok(out)
        })
        .await
    }
}

async fn run_blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::internal(format!("registry proxy worker panicked: {e}")))?
}

fn upstream(url: &str, detail: impl std::fmt::Display) -> ApiError {
    ApiError::upstream(format!("{url}: {detail}"))
}

fn world_not_found(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        matches!(
            c.downcast_ref::<ureq::Error>(),
            Some(ureq::Error::StatusCode(404))
        )
    })
}

fn parse_active_entity(v: &serde_json::Value) -> Option<ActiveEntity> {
    let entity_id = v.get("id")?.as_str()?.to_string();
    Some(ActiveEntity {
        deployment_id: 0,
        entity_id,
        entity_type: v
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        timestamp: v.get("timestamp").and_then(|t| t.as_i64()).unwrap_or(0),
        pointers: v
            .get("pointers")
            .and_then(|p| p.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        metadata: v
            .get("metadata")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        deployer_address: None,
        content: v
            .get("content")
            .and_then(|c| c.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|c| {
                        Some(ContentFile {
                            file: c.get("file")?.as_str()?.to_string(),
                            hash: c.get("hash")?.as_str()?.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_active_entity;

    #[test]
    fn parse_active_entity_maps_catalyst_shape() {
        let v = serde_json::json!({
            "id": "bafkreitest",
            "type": "profile",
            "timestamp": 1782484179697i64,
            "pointers": ["0x24e5f44999c151f08609f8e27b2238c773c4d020"],
            "content": [{"file": "face.png", "hash": "bafkface"}],
            "metadata": {"avatars": []},
            "version": "v3",
        });
        let e = parse_active_entity(&v).unwrap();
        assert_eq!(e.entity_id, "bafkreitest");
        assert_eq!(e.entity_type, "profile");
        assert_eq!(e.timestamp, 1782484179697);
        assert_eq!(
            e.pointers,
            vec!["0x24e5f44999c151f08609f8e27b2238c773c4d020"]
        );
        assert_eq!(e.content.len(), 1);
        assert_eq!(e.content[0].file, "face.png");
        assert_eq!(e.content[0].hash, "bafkface");
        assert_eq!(e.deployment_id, 0);
        assert!(e.deployer_address.is_none());
        assert!(e.metadata.get("avatars").is_some());

        assert!(parse_active_entity(&serde_json::json!({"type": "scene"})).is_none());
    }
}
