use anyhow::{anyhow, Context, Result};
use rayon::prelude::*;
use sha1::{Digest, Sha1};
use std::io::Read;
use std::path::{Path, PathBuf};

const DEFAULT_WORLDS_URL: &str = "https://worlds-content-server.decentraland.org";

fn usage() -> ! {
    eprintln!(
        "usage: abgen-world <world-name>... --store <dir> [--ids-out <file>]\n\
         \n\
         options:\n  \
         --store <dir>       content cache written as <dir>/<sha1(cid)[:4]>/<cid>\n  \
         --ids-out <file>    write resolved scene entity ids (default <store>/entity-ids.txt)\n  \
         --worlds-url <url>  worlds-content-server (default {DEFAULT_WORLDS_URL};\n                      \
                             any worlds-content-server-compatible endpoint works)\n  \
         -j <n>              parallel content downloads (default 16)"
    );
    std::process::exit(2);
}

fn store_path(store: &Path, cid: &str) -> PathBuf {
    let digest = Sha1::digest(cid.as_bytes());
    store
        .join(&format!("{:02x}{:02x}", digest[0], digest[1])[..4])
        .join(cid)
}

fn http_get(url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .call()
        .with_context(|| format!("GET {url}"))?;
    let mut buf = Vec::new();
    resp.into_body()
        .into_reader()
        .take(512 * 1024 * 1024)
        .read_to_end(&mut buf)
        .with_context(|| format!("read body of {url}"))?;
    Ok(buf)
}

fn fetch_to_store(store: &Path, base_url: &str, cid: &str) -> Result<bool> {
    let dst = store_path(store, cid);
    if dst.exists() {
        return Ok(false);
    }
    let data = http_get(&format!("{base_url}{cid}"))?;
    std::fs::create_dir_all(dst.parent().unwrap())?;
    let tmp = dst.with_extension("part");
    std::fs::write(&tmp, &data)?;
    std::fs::rename(&tmp, &dst)?;
    Ok(true)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut worlds: Vec<String> = Vec::new();
    let mut store: Option<PathBuf> = None;
    let mut ids_out: Option<PathBuf> = None;
    let mut worlds_url = DEFAULT_WORLDS_URL.to_string();
    let mut jobs = 16usize;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--store" => {
                i += 1;
                store = Some(PathBuf::from(
                    argv.get(i).cloned().unwrap_or_else(|| usage()),
                ));
            }
            "--ids-out" => {
                i += 1;
                ids_out = Some(PathBuf::from(
                    argv.get(i).cloned().unwrap_or_else(|| usage()),
                ));
            }
            "--worlds-url" => {
                i += 1;
                worlds_url = argv.get(i).cloned().unwrap_or_else(|| usage());
            }
            "-j" | "--jobs" => {
                i += 1;
                jobs = argv
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            "-h" | "--help" => usage(),
            other if other.starts_with("--") => {
                eprintln!("unknown option: {other}");
                usage();
            }
            other => worlds.push(other.to_string()),
        }
        i += 1;
    }
    if worlds.is_empty() {
        usage();
    }
    let store = store.unwrap_or_else(|| usage());
    std::fs::create_dir_all(&store)?;
    let ids_out = ids_out.unwrap_or_else(|| store.join("entity-ids.txt"));
    let worlds_url = worlds_url.trim_end_matches('/').to_string();

    rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build_global()
        .ok();

    let mut entity_ids: Vec<String> = Vec::new();
    for world in &worlds {
        let res: Result<()> = (|| {
        let about_url = format!("{worlds_url}/world/{world}/about");
        let about: serde_json::Value = serde_json::from_slice(&http_get(&about_url)?)
            .with_context(|| format!("parse {about_url}"))?;
        let urns = about
            .pointer("/configurations/scenesUrn")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("{world}: no configurations.scenesUrn in {about_url}"))?;
        if urns.is_empty() {
            eprintln!("{world}: no scenes deployed, skipping");
            return Ok(());
        }
        for urn in urns {
            let urn = urn.as_str().unwrap_or("");

            let after = urn
                .strip_prefix("urn:decentraland:entity:")
                .ok_or_else(|| anyhow!("{world}: unexpected scene urn '{urn}'"))?;
            let (cid, query) = after.split_once('?').unwrap_or((after, ""));
            let base_url = query
                .split('&')
                .find_map(|kv| kv.strip_prefix("baseUrl="))
                .unwrap_or(&format!("{worlds_url}/contents/"))
                .to_string();

            fetch_to_store(&store, &base_url, cid)
                .with_context(|| format!("{world}: fetch entity {cid}"))?;
            let entity: serde_json::Value =
                serde_json::from_slice(&std::fs::read(store_path(&store, cid))?)
                    .with_context(|| format!("{world}: parse entity {cid}"))?;
            let content: Vec<(String, String)> = entity
                .get("content")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| {
                            Some((
                                c.get("file")?.as_str()?.to_string(),
                                c.get("hash")?.as_str()?.to_string(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();

            let fetched: usize = content
                .par_iter()
                .map(
                    |(file, hash)| match fetch_to_store(&store, &base_url, hash) {
                        Ok(new) => usize::from(new),
                        Err(e) => {
                            eprintln!("{world}: {file} ({hash}): {e:#}");
                            0
                        }
                    },
                )
                .sum();
            eprintln!(
                "{world}: entity {cid} — {} content files ({fetched} downloaded, rest cached)",
                content.len()
            );
            entity_ids.push(cid.to_string());
        }
        Ok(())
        })();
        if let Err(e) = res {
            eprintln!("skip {world}: {e:#}");
        }
    }

    if entity_ids.is_empty() {
        return Err(anyhow!("no scene entities resolved"));
    }
    std::fs::write(&ids_out, entity_ids.join("\n") + "\n")?;
    eprintln!(
        "wrote {} entity id(s) to {}\nnext:\n  abgen-corpus --entity-ids {} <out-dir> --content-dir {} --cdn-layout --real-textures --v38-compat",
        entity_ids.len(),
        ids_out.display(),
        ids_out.display(),
        store.display()
    );
    Ok(())
}
