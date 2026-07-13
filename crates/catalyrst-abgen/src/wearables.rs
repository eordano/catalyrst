use crate::builder::{build_bundle, BuildOpts};
use crate::catalyst::{CatalystClient, Scene};
use crate::{manifest, naming};
use anyhow::Result;
use rayon::prelude::*;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct EntityResult {
    pub entity_id: String,

    pub urn: Option<String>,

    pub bundle_count: usize,

    pub total_bytes: usize,

    pub skipped: Vec<(String, String)>,
}

#[derive(Debug, Default)]
pub struct CollectionResult {
    pub urn: String,

    pub was_collection: bool,

    pub entities: Vec<EntityResult>,

    pub unresolved: Vec<String>,
}

impl CollectionResult {
    pub fn total_bundles(&self) -> usize {
        self.entities.iter().map(|e| e.bundle_count).sum()
    }
    pub fn total_bytes(&self) -> usize {
        self.entities.iter().map(|e| e.total_bytes).sum()
    }
}

#[derive(Clone, Debug)]
pub struct ConvertOptions {
    pub platform: String,
    pub ab_version: String,
    pub keep_forward_plus: bool,

    pub per_entity_limit: Option<usize>,

    pub collection_mode: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        ConvertOptions {
            platform: "windows".to_string(),
            ab_version: manifest::DEFAULT_AB_VERSION.to_string(),
            keep_forward_plus: true,
            per_entity_limit: None,
            collection_mode: false,
        }
    }
}

pub fn default_lambdas_base(content_base: &str) -> String {
    let b = content_base.trim_end_matches('/');
    if let Some(prefix) = b.strip_suffix("/content") {
        return format!("{prefix}/lambdas");
    }
    format!("{b}/lambdas")
}

fn resolve_member_urns(
    client: &CatalystClient,
    lambdas_base: &str,
    urn: &str,
) -> (Vec<String>, bool) {
    match client.collection_member_urns(lambdas_base, urn) {
        Ok(urns) if !urns.is_empty() => (urns, true),

        _ => (vec![urn.to_string()], false),
    }
}

pub fn convert_entity(
    client: &CatalystClient,
    entity: &Scene,
    out_dir: &str,
    opts: &ConvertOptions,
) -> Result<EntityResult> {
    let mut glbs = entity.files_with_ext(&naming::GLTF_EXTENSIONS);
    if let Some(n) = opts.per_entity_limit {
        glbs.truncate(n);
    }
    let content_by_file = entity.content_by_file();
    let keep_fp = opts.keep_forward_plus;
    let collection_mode = opts.collection_mode;
    let platform = opts.platform.clone();
    let entity_type = entity.entity_type.clone();

    let results: Vec<(String, std::result::Result<Vec<u8>, String>)> = glbs
        .par_iter()
        .map(|c| {
            let f = &c.file;
            let h = &c.hash;
            let built = (|| -> Result<(String, Vec<u8>)> {
                let glb = client.fetch_content(h)?;
                let ext = naming::file_extension(f);
                let digest = naming::deps_digest_for_glb(&glb, f, &content_by_file, false)?;
                let name = naming::canonical_filename(h, &ext, &platform, Some(&digest))?;

                let resolve_fn = |uri: &str| -> Option<Vec<u8>> {
                    let key = naming::resolve_uri_to_content_file(uri, f)
                        .ok()?
                        .to_lowercase();
                    let content_hash = content_by_file.get(&key)?;
                    client.fetch_content(content_hash).ok()
                };
                let resolve: crate::gltf::Resolve = Some(&resolve_fn);
                let resolve_hash_fn = |uri: &str| -> Option<String> {
                    let key = naming::resolve_uri_to_content_file(uri, f)
                        .ok()?
                        .to_lowercase();
                    content_by_file.get(&key).cloned()
                };
                let opts = BuildOpts {
                    keep_forward_plus: keep_fp,
                    source_file: Some(f),

                    entity_type: if entity_type.is_empty() {
                        None
                    } else {
                        Some(entity_type.as_str())
                    },
                    resolve,
                    resolve_hash: Some(&resolve_hash_fn),
                    collection_mode,
                    ..Default::default()
                };
                let data = build_bundle(&glb, &name, h, &opts)?.data;
                Ok((name, data))
            })();
            match built {
                Ok((name, data)) => (name, Ok(data)),
                Err(e) => (f.clone(), Err(format!("{e:#}"))),
            }
        })
        .collect();

    let mut bundles: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut skipped: Vec<(String, String)> = Vec::new();
    for (name, res) in results {
        match res {
            Ok(data) => {
                bundles.entry(name).or_insert(data);
            }
            Err(why) => skipped.push((name, why)),
        }
    }

    let total_bytes: usize = bundles.values().map(|b| b.len()).sum();
    let bundle_count = bundles.len();

    manifest::write_scene(
        out_dir,
        &entity.entity_id,
        &opts.platform,
        &bundles,
        &opts.ab_version,
        manifest::exit_code_for_failures(skipped.len()),
        &crate::live::build_scoped_date(),
    )?;

    Ok(EntityResult {
        entity_id: entity.entity_id.clone(),
        urn: entity.pointers.first().cloned(),
        bundle_count,
        total_bytes,
        skipped,
    })
}

pub fn convert_collection(
    client: &CatalystClient,
    urn: &str,
    out_dir: &str,
    opts: &ConvertOptions,
    lambdas_base: Option<&str>,
) -> Result<CollectionResult> {
    let opts = &ConvertOptions {
        collection_mode: true,
        ..opts.clone()
    };

    let lambdas = lambdas_base
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_lambdas_base(client.base_url()));

    let (member_urns, was_collection) = resolve_member_urns(client, &lambdas, urn);
    eprintln!(
        "{} {:?} -> {} member URN(s){}",
        if was_collection {
            "collection"
        } else {
            "wearable"
        },
        urn,
        member_urns.len(),
        if was_collection {
            format!(" (lambdas {lambdas})")
        } else {
            String::new()
        }
    );

    let entities = client.resolve_entities(&member_urns)?;

    let mut resolved_pointers: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in &entities {
        for p in &e.pointers {
            resolved_pointers.insert(p.to_lowercase());
        }
    }
    let unresolved: Vec<String> = member_urns
        .iter()
        .filter(|u| !resolved_pointers.contains(&u.to_lowercase()))
        .cloned()
        .collect();

    let mut out = CollectionResult {
        urn: urn.to_string(),
        was_collection,
        entities: Vec::with_capacity(entities.len()),
        unresolved,
    };

    for entity in &entities {
        match convert_entity(client, entity, out_dir, opts) {
            Ok(res) => {
                eprintln!(
                    "  {} ({})  {} bundle(s), {:.1} MB{}",
                    res.entity_id,
                    res.urn.as_deref().unwrap_or("?"),
                    res.bundle_count,
                    res.total_bytes as f64 / 1e6,
                    if res.skipped.is_empty() {
                        String::new()
                    } else {
                        format!(", {} skipped", res.skipped.len())
                    }
                );
                out.entities.push(res);
            }
            Err(e) => {
                eprintln!("  {} FAILED: {e:#}", entity.entity_id);
                out.entities.push(EntityResult {
                    entity_id: entity.entity_id.clone(),
                    urn: entity.pointers.first().cloned(),
                    bundle_count: 0,
                    total_bytes: 0,
                    skipped: vec![("<entity>".to_string(), format!("{e:#}"))],
                });
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lambdas_base_derivation() {
        assert_eq!(
            default_lambdas_base("http://localhost:5141/content"),
            "http://localhost:5141/lambdas"
        );
        assert_eq!(
            default_lambdas_base("http://localhost:5141/content/"),
            "http://localhost:5141/lambdas"
        );
        assert_eq!(
            default_lambdas_base("https://peer.decentraland.org/content"),
            "https://peer.decentraland.org/lambdas"
        );
        assert_eq!(
            default_lambdas_base("https://peer.decentraland.org/content/"),
            "https://peer.decentraland.org/lambdas"
        );
    }
}
