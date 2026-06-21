//! Scene source acquisition. Port of `src/logic/sceneFetcher.ts`.
//!
//! Two paths, both ending in the scene's compiled `game.js` source:
//!
//! - **local** — read `LOCAL_SCENE_PATH` straight off disk (dev mode).
//! - **world** — resolve a world name against a worlds content-server:
//!   1. `GET {WORLD_SERVER_URL}/world/{name}/about`; require `healthy == true`.
//!   2. Parse `configurations.scenesUrn[0]` — a `urn:...:<sceneHash>?baseUrl=...`.
//!      `sceneHash` is the 3rd `:`-delimited segment of the URN path; `baseUrl`
//!      is the query param.
//!   3. `GET {baseUrl}{sceneHash}` -> scene entity JSON; find the `content`
//!      entry whose `file == metadata.main` to get the entry-point hash.
//!   4. `GET {baseUrl}{entryHash}` -> the JS source.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

pub async fn from_local(path: &str) -> Result<String> {
    tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("read local scene {path}"))
}

#[derive(Debug)]
pub struct WorldScene {
    pub scene_hash: String,
    pub code: String,
    /// The scene's static `main.crdt` (composer output: the static entities/
    /// components). Empty if the scene ships none. Seeded into the CRDT engine
    /// at load so connecting clients see the scene geometry, not an empty world.
    pub static_crdt: Vec<u8>,
}

#[derive(Deserialize)]
struct About {
    healthy: bool,
    configurations: Configurations,
}

#[derive(Deserialize)]
struct Configurations {
    #[serde(rename = "scenesUrn")]
    scenes_urn: Vec<String>,
}

#[derive(Deserialize)]
struct SceneEntity {
    metadata: SceneMetadata,
    content: Vec<ContentEntry>,
}

#[derive(Deserialize)]
struct SceneMetadata {
    main: String,
}

#[derive(Deserialize)]
struct ContentEntry {
    file: String,
    hash: String,
}

pub async fn from_world(
    client: &reqwest::Client,
    world_server_url: &str,
    world_name: &str,
) -> Result<WorldScene> {
    let about: About = client
        .get(format!("{world_server_url}/world/{world_name}/about"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("parse /about")?;
    if !about.healthy {
        return Err(anyhow!(
            "world content server {world_server_url} is unhealthy"
        ));
    }

    let urn = about
        .configurations
        .scenes_urn
        .first()
        .ok_or_else(|| anyhow!("no scenesUrn in /about"))?;
    let parsed = url::Url::parse(urn).context("parse scenesUrn")?;
    // urn:decentraland:entity:<sceneHash>?baseUrl=... — hash is path segment [2].
    let scene_hash = parsed
        .path()
        .split(':')
        .nth(2)
        .ok_or_else(|| anyhow!("malformed scenesUrn {urn}"))?
        .to_string();
    let base_url = parsed
        .query_pairs()
        .find(|(k, _)| k == "baseUrl")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| anyhow!("scenesUrn missing baseUrl"))?;

    let scene: SceneEntity = client
        .get(format!("{base_url}{scene_hash}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("parse scene entity")?;

    let entry = scene
        .content
        .iter()
        .find(|c| c.file == scene.metadata.main)
        .ok_or_else(|| anyhow!("cannot find entry point for scene"))?;

    let code = client
        .get(format!("{base_url}{}", entry.hash))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
        .context("fetch scene code")?;

    // Fetch the optional static main.crdt (the scene composer's static entities).
    let static_crdt = match scene.content.iter().find(|c| c.file == "main.crdt") {
        Some(c) => client
            .get(format!("{base_url}{}", c.hash))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await
            .context("fetch main.crdt")?
            .to_vec(),
        None => Vec::new(),
    };

    Ok(WorldScene {
        scene_hash,
        code,
        static_crdt,
    })
}
