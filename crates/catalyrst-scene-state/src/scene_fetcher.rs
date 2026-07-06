use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
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

    pub static_crdt: Vec<u8>,

    // metadata.scene.base of the deployed entity — what world-storage derives as
    // the scene's parcel, so it is what storage delegations get scoped to.
    pub base_parcel: String,
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
    #[serde(default)]
    scene: SceneSpec,
}

#[derive(Deserialize, Default)]
struct SceneSpec {
    #[serde(default)]
    base: String,
}

#[derive(Deserialize)]
struct ContentEntry {
    file: String,
    hash: String,
}

pub(crate) async fn read_body_capped(resp: reqwest::Response, max_bytes: usize) -> Result<Vec<u8>> {
    if let Some(len) = resp.content_length() {
        if len > max_bytes as u64 {
            return Err(anyhow!(
                "response body advertises {len} bytes, over the {max_bytes} byte cap"
            ));
        }
    }
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if buf.len().saturating_add(chunk.len()) > max_bytes {
            return Err(anyhow!("response body exceeds the {max_bytes} byte cap"));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

async fn get_capped(client: &reqwest::Client, url: String, max_bytes: usize) -> Result<Vec<u8>> {
    let resp = client.get(&url).send().await?.error_for_status()?;
    read_body_capped(resp, max_bytes)
        .await
        .with_context(|| format!("fetch {url}"))
}

pub async fn from_world(
    client: &reqwest::Client,
    world_server_url: &str,
    world_name: &str,
    max_body_bytes: usize,
) -> Result<WorldScene> {
    let body = get_capped(
        client,
        format!("{world_server_url}/world/{world_name}/about"),
        max_body_bytes,
    )
    .await?;
    let about: About = serde_json::from_slice(&body).context("parse /about")?;
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

    let body = get_capped(client, format!("{base_url}{scene_hash}"), max_body_bytes).await?;
    let scene: SceneEntity = serde_json::from_slice(&body).context("parse scene entity")?;

    let entry = scene
        .content
        .iter()
        .find(|c| c.file == scene.metadata.main)
        .ok_or_else(|| anyhow!("cannot find entry point for scene"))?;

    let code_bytes = get_capped(client, format!("{base_url}{}", entry.hash), max_body_bytes)
        .await
        .context("fetch scene code")?;
    // leading BOM stripped for parity with the reqwest .text() decode this replaced
    let code = String::from_utf8_lossy(
        code_bytes
            .strip_prefix(b"\xef\xbb\xbf".as_slice())
            .unwrap_or(&code_bytes),
    )
    .into_owned();

    let static_crdt = match scene.content.iter().find(|c| c.file == "main.crdt") {
        Some(c) => get_capped(client, format!("{base_url}{}", c.hash), max_body_bytes)
            .await
            .context("fetch main.crdt")?,
        None => Vec::new(),
    };

    let base_parcel = if scene.metadata.scene.base.is_empty() {
        "0,0".to_string()
    } else {
        scene.metadata.scene.base.clone()
    };

    Ok(WorldScene {
        scene_hash,
        code,
        static_crdt,
        base_parcel,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn serve_once(response: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let _ = sock.read(&mut [0u8; 1024]).await;
            let _ = sock.write_all(&response).await;
            let _ = sock.shutdown().await;
        });
        format!("http://{addr}/")
    }

    #[tokio::test]
    async fn accepts_body_under_cap() {
        let url = serve_once(b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\n\r\nhello".to_vec()).await;
        let body = get_capped(&reqwest::Client::new(), url, 1024)
            .await
            .unwrap();
        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn rejects_oversized_content_length() {
        let url = serve_once(b"HTTP/1.1 200 OK\r\ncontent-length: 2048\r\n\r\n".to_vec()).await;
        let err = get_capped(&reqwest::Client::new(), url, 1024)
            .await
            .unwrap_err();
        assert!(format!("{err:#}").contains("byte cap"), "{err:#}");
    }

    #[tokio::test]
    async fn rejects_streamed_overflow() {
        let mut resp = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n".to_vec();
        for _ in 0..4 {
            resp.extend_from_slice(b"200\r\n");
            resp.extend_from_slice(&[b'x'; 512]);
            resp.extend_from_slice(b"\r\n");
        }
        resp.extend_from_slice(b"0\r\n\r\n");
        let url = serve_once(resp).await;
        let err = get_capped(&reqwest::Client::new(), url, 1024)
            .await
            .unwrap_err();
        assert!(format!("{err:#}").contains("byte cap"), "{err:#}");
    }
}
