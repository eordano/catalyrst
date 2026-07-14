use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Value};

fn read_json(path: &Path) -> Result<Value> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

fn subdirs_with_data(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("read dir {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && p.join("data.json").exists())
        .collect();
    out.sort();
    Ok(out)
}

fn asset_files(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(base, &path, out)?;
            } else {
                let rel = path
                    .strip_prefix(base)?
                    .to_string_lossy()
                    .replace('\\', "/");
                if rel != "data.json" {
                    out.push((rel, path));
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn model_from_composite(composite: &Value) -> Option<String> {
    for component in composite.get("components")?.as_array()? {
        if component.get("name").and_then(Value::as_str) == Some("core::GltfContainer") {
            let src = component
                .get("data")?
                .get("0")?
                .get("json")?
                .get("src")?
                .as_str()?;
            return Some(src.strip_prefix("{assetPath}/").unwrap_or(src).to_string());
        }
    }
    None
}

pub fn run(packs_dir: &str, out_dir: &str) -> Result<()> {
    let out = Path::new(out_dir);
    fs::create_dir_all(out).with_context(|| format!("create out dir {out_dir}"))?;

    let mut packs_json: Vec<Value> = Vec::new();
    let mut stored = 0usize;
    let mut total_assets = 0usize;

    for pack_dir in subdirs_with_data(Path::new(packs_dir))? {
        let pack_data = read_json(&pack_dir.join("data.json"))?;
        let title = pack_data
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let assets_dir = pack_dir.join("assets");
        if !assets_dir.is_dir() {
            continue;
        }

        let mut assets_json: Vec<Value> = Vec::new();
        for asset_dir in subdirs_with_data(&assets_dir)? {
            if asset_dir
                .to_string_lossy()
                .contains("/admin_toolkit/assets/")
            {
                continue;
            }
            let data = read_json(&asset_dir.join("data.json"))?;
            let composite = read_json(&asset_dir.join("composite.json")).unwrap_or(Value::Null);

            let mut contents: BTreeMap<String, String> = BTreeMap::new();
            for (rel, abs) in asset_files(&asset_dir)? {
                let bytes = fs::read(&abs)?;
                let hash = catalyrst_hashing::hash_bytes_v1(&bytes);
                let dest = out.join(&hash);
                if !dest.exists() {
                    fs::write(&dest, &bytes)
                        .with_context(|| format!("write content {}", dest.display()))?;
                    stored += 1;
                }
                contents.insert(rel, hash);
            }

            let model = model_from_composite(&composite).or_else(|| {
                contents
                    .keys()
                    .find(|k| k.ends_with(".glb") || k.ends_with(".gltf"))
                    .cloned()
            });
            let model = match model {
                Some(m) if contents.contains_key(&m) => m,
                _ => continue,
            };

            let mut asset = data.as_object().cloned().unwrap_or_default();
            asset.insert("model".into(), json!(model));
            if contents.contains_key("thumbnail.png") {
                asset.insert("thumbnail".into(), json!("thumbnail.png"));
            }
            asset.insert("contents".into(), json!(contents));
            if !composite.is_null() {
                asset.insert("composite".into(), composite);
            }
            assets_json.push(Value::Object(asset));
        }

        total_assets += assets_json.len();
        packs_json.push(json!({ "title": title, "assets": assets_json }));
    }

    let catalog = json!({ "data": packs_json });
    let catalog_path = out.join("catalog.json");
    fs::write(&catalog_path, serde_json::to_vec_pretty(&catalog)?)
        .with_context(|| format!("write {}", catalog_path.display()))?;

    println!(
        "catalog: {} packs, {} assets, {} content files -> {}",
        packs_json.len(),
        total_assets,
        stored,
        catalog_path.display()
    );
    Ok(())
}
