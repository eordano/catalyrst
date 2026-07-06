use crate::local_store::LocalContentStore;
use crate::{anyhow, Context, Result};
use rayon::prelude::*;
use std::io::Read;
use std::net::{IpAddr, ToSocketAddrs};

pub const WORLDS_URL_ENV: &str = "ABGEN_WORLDS_URL";
pub const WORLDS_CONTENT_FALLBACK_ENV: &str = "ABGEN_WORLDS_CONTENT_URL";
pub const DEFAULT_WORLDS_URL: &str = "https://worlds-content-server.decentraland.org";

pub fn parse_content_fallback(raw: Option<&str>) -> Option<String> {
    match raw {
        None => Some(DEFAULT_WORLDS_URL.to_string()),
        Some(v) => {
            let v = v.trim();
            if v.is_empty() || crate::clihelp::bool_token(v) == Some(false) {
                None
            } else {
                Some(v.trim_end_matches('/').to_string())
            }
        }
    }
}

pub fn content_fallback_from_env() -> Option<String> {
    let raw = std::env::var(WORLDS_CONTENT_FALLBACK_ENV).ok();
    parse_content_fallback(raw.as_deref())
}

pub struct WorldScene {
    pub entity_id: String,
    pub base_url: String,
}

pub fn worlds_url_from_env() -> String {
    std::env::var(WORLDS_URL_ENV)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_WORLDS_URL.to_string())
}

pub const SERVE_FETCH_TIMEOUT_SECS: u64 = 20;

const MAX_BODY_BYTES: u64 = 512 * 1024 * 1024;

fn read_capped(reader: &mut impl Read, cap: u64, url: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    reader
        .take(cap + 1)
        .read_to_end(&mut buf)
        .with_context(|| format!("read body of {url}"))?;
    if buf.len() as u64 > cap {
        return Err(anyhow!(
            "body of {url} exceeds the {cap}-byte cap; refusing to truncate"
        ));
    }
    Ok(buf)
}

fn http_get_timeout(url: &str, timeout_secs: u64) -> Result<Vec<u8>> {
    let resp = ureq::get(url)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(timeout_secs)))
        .max_redirects(0)
        .build()
        .call()
        .with_context(|| format!("GET {url}"))?;
    read_capped(&mut resp.into_body().into_reader(), MAX_BODY_BYTES, url)
}

fn http_get(url: &str) -> Result<Vec<u8>> {
    http_get_timeout(url, 120)
}

pub fn parse_scene_urn(urn: &str, worlds_url: &str) -> Result<WorldScene> {
    let after = urn
        .strip_prefix("urn:decentraland:entity:")
        .ok_or_else(|| anyhow!("unexpected scene urn '{urn}'"))?;
    let (cid, query) = after.split_once('?').unwrap_or((after, ""));
    if cid.is_empty() {
        return Err(anyhow!("empty entity id in scene urn '{urn}'"));
    }
    let base_url = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("baseUrl="))
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}/contents/", worlds_url.trim_end_matches('/')));
    Ok(WorldScene {
        entity_id: cid.to_string(),
        base_url,
    })
}

pub fn scenes_from_about(about: &serde_json::Value, worlds_url: &str) -> Result<Vec<WorldScene>> {
    let urns = about
        .pointer("/configurations/scenesUrn")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("no configurations.scenesUrn in about payload"))?;
    urns.iter()
        .map(|u| parse_scene_urn(u.as_str().unwrap_or(""), worlds_url))
        .collect()
}

pub fn resolve_world(worlds_url: &str, name: &str) -> Result<Vec<WorldScene>> {
    resolve_world_bounded(worlds_url, name, 120)
}

pub fn resolve_world_bounded(
    worlds_url: &str,
    name: &str,
    timeout_secs: u64,
) -> Result<Vec<WorldScene>> {
    let about_url = format!("{}/world/{name}/about", worlds_url.trim_end_matches('/'));
    let about: serde_json::Value =
        serde_json::from_slice(&http_get_timeout(&about_url, timeout_secs)?)
            .with_context(|| format!("parse {about_url}"))?;
    scenes_from_about(&about, worlds_url).with_context(|| about_url)
}

fn ip_is_blocked(ip: &IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_private() || v4.is_link_local() || (o[0] == 100 && (o[1] & 0xc0) == 64)
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
        }
    }
}

fn guard_base_url(url: &str) -> Result<()> {
    guard_base_url_with(
        url,
        crate::clihelp::env_bool("ABGEN_ALLOW_PRIVATE_BASE_URL", false),
    )
}

fn guard_base_url_with(url: &str, allow_private: bool) -> Result<()> {
    if allow_private {
        return Ok(());
    }
    let lower = url.to_ascii_lowercase();
    let (rest, default_port) = if let Some(rest) = lower.strip_prefix("https://") {
        (rest, 443u16)
    } else if let Some(rest) = lower.strip_prefix("http://") {
        (rest, 80u16)
    } else {
        return Err(anyhow!("refusing base_url {url}: non-http(s) scheme"));
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let authority = match authority.rsplit_once('@') {
        Some((_, after)) => after,
        None => authority,
    };
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let (h, tail) = inner
            .split_once(']')
            .ok_or_else(|| anyhow!("refusing base_url {url}: malformed ipv6 authority"))?;
        let port = match tail.strip_prefix(':') {
            Some(p) => p.parse::<u16>().unwrap_or(default_port),
            None => default_port,
        };
        (h, port)
    } else if let Some((h, p)) = authority.split_once(':') {
        (h, p.parse::<u16>().unwrap_or(default_port))
    } else {
        (authority, default_port)
    };
    let addrs = match (host, port).to_socket_addrs() {
        Ok(addrs) => addrs,
        Err(_) => return Ok(()),
    };
    for addr in addrs {
        let ip = match addr.ip() {
            IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
                Some(v4) => IpAddr::V4(v4),
                None => IpAddr::V6(v6),
            },
            v4 => v4,
        };
        if ip_is_blocked(&ip) {
            return Err(anyhow!(
                "refusing base_url {url}: resolves to blocked address {ip}"
            ));
        }
    }
    Ok(())
}

pub fn fetch_scene_entity(scene: &WorldScene, timeout_secs: u64) -> Result<serde_json::Value> {
    let url = format!("{}{}", scene.base_url, scene.entity_id);
    guard_base_url(&url)?;
    let mut v: serde_json::Value = serde_json::from_slice(&http_get_timeout(&url, timeout_secs)?)
        .with_context(|| format!("parse entity {url}"))?;
    crate::catalyst::ensure_entity_id(&mut v, &scene.entity_id);
    Ok(v)
}

fn fetch_to_store(store: &LocalContentStore, base_url: &str, cid: &str) -> Result<bool> {
    if store.exists(cid) {
        return Ok(false);
    }
    let data = http_get(&format!("{base_url}{cid}"))?;
    store.write(cid, &data)?;
    Ok(true)
}

pub fn fetch_scene_into_store(
    store: &LocalContentStore,
    scene: &WorldScene,
) -> Result<(usize, usize)> {
    guard_base_url(&scene.base_url)?;
    fetch_to_store(store, &scene.base_url, &scene.entity_id)
        .with_context(|| format!("fetch entity {}", scene.entity_id))?;
    let entity: serde_json::Value = serde_json::from_slice(&store.fetch(&scene.entity_id)?)
        .with_context(|| format!("parse entity {}", scene.entity_id))?;
    let hashes: Vec<String> = entity
        .get("content")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| Some(c.get("hash")?.as_str()?.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let fetched: usize = hashes
        .par_iter()
        .map(|hash| match fetch_to_store(store, &scene.base_url, hash) {
            Ok(new) => usize::from(new),
            Err(e) => {
                eprintln!("{}: {hash}: {e:#}", scene.entity_id);
                0
            }
        })
        .sum();
    Ok((fetched, hashes.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_capped_refuses_truncation() {
        let mut small = std::io::Cursor::new(vec![7u8; 10]);
        let got = read_capped(&mut small, 10, "u").unwrap();
        assert_eq!(got.len(), 10);
        let mut big = std::io::Cursor::new(vec![7u8; 11]);
        let err = read_capped(&mut big, 10, "u").unwrap_err();
        assert!(err.to_string().contains("refusing to truncate"), "{err}");
    }

    #[test]
    fn content_fallback_parsing() {
        assert_eq!(
            parse_content_fallback(None),
            Some(DEFAULT_WORLDS_URL.to_string())
        );
        assert_eq!(parse_content_fallback(Some("")), None);
        assert_eq!(parse_content_fallback(Some("off")), None);
        assert_eq!(parse_content_fallback(Some("0")), None);
        assert_eq!(
            parse_content_fallback(Some("https://w.example/")),
            Some("https://w.example".to_string())
        );
    }

    #[test]
    fn urn_with_base_url_query() {
        let s = parse_scene_urn(
            "urn:decentraland:entity:bafkreiabc?=&baseUrl=https://example.org/contents/",
            DEFAULT_WORLDS_URL,
        )
        .unwrap();
        assert_eq!(s.entity_id, "bafkreiabc");
        assert_eq!(s.base_url, "https://example.org/contents/");
    }

    #[test]
    fn urn_without_query_uses_worlds_url_default() {
        let s = parse_scene_urn("urn:decentraland:entity:bafkreixyz", "https://w.example").unwrap();
        assert_eq!(s.entity_id, "bafkreixyz");
        assert_eq!(s.base_url, "https://w.example/contents/");
    }

    #[test]
    fn non_entity_urn_is_rejected() {
        assert!(parse_scene_urn("urn:decentraland:off-chain:x", DEFAULT_WORLDS_URL).is_err());
    }

    #[test]
    fn guard_base_url_rejects_private_and_honors_escape() {
        for url in [
            "http://127.0.0.1:8000/contents/",
            "http://10.0.0.5/contents/",
            "https://169.254.169.254/contents/",
            "http://100.100.100.100/contents/",
            "http://0.0.0.0/contents/",
            "http://[::1]/contents/",
            "http://[fc00::1]/contents/",
            "http://[fe80::1]/contents/",
            "ftp://1.1.1.1/contents/",
        ] {
            assert!(
                guard_base_url_with(url, false).is_err(),
                "{url} should be refused"
            );
        }
        for url in [
            "http://1.1.1.1/contents/",
            "https://8.8.8.8:443/contents/",
            "http://[2606:4700:4700::1111]/contents/",
        ] {
            assert!(guard_base_url_with(url, false).is_ok(), "{url} should pass");
        }
        assert!(guard_base_url_with("http://127.0.0.1:8000/contents/", true).is_ok());
    }

    #[test]
    fn about_payload_yields_scenes() {
        let about: serde_json::Value = serde_json::from_str(
            r#"{"configurations":{"scenesUrn":["urn:decentraland:entity:bafkreiabc?=&baseUrl=https://w.example/contents/"]}}"#,
        )
        .unwrap();
        let scenes = scenes_from_about(&about, DEFAULT_WORLDS_URL).unwrap();
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[0].entity_id, "bafkreiabc");
    }
}
