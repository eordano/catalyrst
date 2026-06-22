use crate::builder::{build_bundle, BuildOpts};
use crate::catalyst::CatalystClient;
use crate::{compress, naming};
use anyhow::{anyhow, bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const DEFAULT_LODS_BUCKET: &str = "https://lods-bucket-ed4300a.s3.amazonaws.com";

#[derive(Clone, Debug)]
pub struct LodOptions {
    pub platform: String,

    pub ab_version: String,

    pub keep_forward_plus: bool,
}

impl Default for LodOptions {
    fn default() -> Self {
        LodOptions {
            platform: "windows".to_string(),
            ab_version: crate::manifest::DEFAULT_AB_VERSION.to_string(),
            keep_forward_plus: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LodSource {
    pub scene_id: String,

    pub level: u32,

    pub origin: String,

    pub ext: String,
}

#[derive(Debug)]
pub struct LodResult {
    pub scene_id: String,
    pub level: u32,

    pub bundle_name: String,

    pub bytes: usize,

    pub rel_path: String,
}

#[derive(Debug, Default)]
pub struct LodConversion {
    pub scene_id: String,
    pub results: Vec<LodResult>,

    pub skipped: Vec<(String, String)>,
}

impl LodConversion {
    pub fn total_bytes(&self) -> usize {
        self.results.iter().map(|r| r.bytes).sum()
    }
}

pub fn parse_lod_filename(locator: &str) -> Result<(String, u32, String)> {
    let no_query = locator.split(['?', '#']).next().unwrap_or(locator);
    let file = no_query.rsplit('/').next().unwrap_or(no_query);
    let ext = naming::file_extension(file);
    let stem = match file.rfind('.') {
        Some(i) => &file[..i],
        None => file,
    };

    let underscore = stem
        .rfind('_')
        .ok_or_else(|| anyhow!("LOD source name {file:?} has no _<level> suffix"))?;
    let (scene_part, level_part) = stem.split_at(underscore);
    let level_str = &level_part[1..];
    let level: u32 = level_str
        .parse()
        .map_err(|_| anyhow!("LOD source name {file:?} has non-numeric level {level_str:?}"))?;
    if scene_part.is_empty() {
        bail!("LOD source name {file:?} has empty scene id");
    }

    Ok((scene_part.to_lowercase(), level, ext))
}

pub fn lod_bundle_name(scene_id: &str, level: u32, platform: &str) -> String {
    format!("{}_{}_{}", scene_id.to_lowercase(), level, platform)
}

fn lod_rel_path(level: u32, bundle_name: &str) -> String {
    format!("LOD/{level}/{bundle_name}")
}

fn fetch_lod_bytes(client: &CatalystClient, locator: &str) -> Result<Vec<u8>> {
    if locator.starts_with("http://") || locator.starts_with("https://") {
        return http_get(locator).with_context(|| format!("download LOD {locator}"));
    }
    let p = Path::new(locator);
    if p.exists() {
        return std::fs::read(p).with_context(|| format!("read LOD file {locator}"));
    }

    client
        .fetch_content(locator)
        .with_context(|| format!("fetch LOD content {locator}"))
}

fn http_get(url: &str) -> Result<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .into();
    let resp = agent
        .get(url)
        .header("User-Agent", crate::catalyst::UA)
        .call()
        .map_err(|e| anyhow!("GET {url}: {e}"))?;
    let mut buf = Vec::new();
    use std::io::Read;
    resp.into_body().into_reader().read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn convert_lods(
    client: &CatalystClient,
    sources: &[String],
    out_dir: &str,
    opts: &LodOptions,
) -> Result<LodConversion> {
    if sources.is_empty() {
        bail!("no LOD sources given");
    }
    let mut conv = LodConversion::default();

    let mut written: BTreeMap<String, (String, Vec<u8>)> = BTreeMap::new();
    let mut scene_id: Option<String> = None;

    for locator in sources {
        let built = (|| -> Result<(LodResult, Vec<u8>)> {
            let (sid, level, ext) = parse_lod_filename(locator)?;
            if ext == ".fbx" {
                bail!(
                    "FBX LOD source {locator:?}: FBX -> GLB transcoder not yet implemented \
                     in the pure-Rust port (TODO: add an FBX importer crate or shell out to \
                     a converter; track in the abgen-rs issue tracker). Workaround: \
                     re-export the asset as .glb upstream and re-run."
                );
            }
            if ext != ".glb" && ext != ".gltf" {
                bail!("unsupported LOD source extension {ext:?} for {locator:?}");
            }

            let glb = fetch_lod_bytes(client, locator)?;

            let bundle_name = lod_bundle_name(&sid, level, &opts.platform);

            let root_hash = format!("{}_{}", sid, level);
            let build_opts = BuildOpts {
                keep_forward_plus: opts.keep_forward_plus,
                source_file: Some(locator),
                ..Default::default()
            };
            let data = build_bundle(&glb, &bundle_name, &root_hash, &build_opts)
                .with_context(|| format!("build LOD bundle for {locator:?}"))?
                .data;

            let rel = lod_rel_path(level, &bundle_name);
            let result = LodResult {
                scene_id: sid,
                level,
                bytes: data.len(),
                rel_path: rel,
                bundle_name,
            };
            Ok((result, data))
        })();

        match built {
            Ok((r, data)) => {
                scene_id.get_or_insert_with(|| r.scene_id.clone());
                written.insert(r.bundle_name.clone(), (r.rel_path.clone(), data));
                conv.results.push(r);
            }
            Err(e) => conv.skipped.push((locator.clone(), format!("{e:#}"))),
        }
    }

    let sid = scene_id
        .clone()
        .ok_or_else(|| anyhow!("no LOD source converted ({} skipped)", conv.skipped.len()))?;
    conv.scene_id = sid.clone();

    let entity_dir = PathBuf::from(out_dir).join(&sid);
    for (rel, data) in written.values() {
        let path = entity_dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data)?;
        write_brotli_sidecar(&path, data)?;
    }

    write_lod_manifest(&entity_dir, &conv, &opts.ab_version)?;
    Ok(conv)
}

pub fn s3_source_urls(
    bucket: &str,
    parcel: &str,
    timestamp: &str,
    scene_id: &str,
    levels: &[u32],
    ext: &str,
) -> Vec<String> {
    let bucket = bucket.trim_end_matches('/');
    let ext = ext.trim_start_matches('.');
    levels
        .iter()
        .map(|lvl| format!("{bucket}/{parcel}/LOD/Sources/{timestamp}/{scene_id}_{lvl}.{ext}"))
        .collect()
}

fn write_brotli_sidecar(path: &Path, data: &[u8]) -> Result<()> {
    let mut br = path.as_os_str().to_owned();
    br.push(".br");
    std::fs::write(PathBuf::from(br), compress::brotli(data)?)?;
    Ok(())
}

fn write_lod_manifest(entity_dir: &Path, conv: &LodConversion, ab_version: &str) -> Result<()> {
    std::fs::create_dir_all(entity_dir)?;
    let mut files: Vec<serde_json::Value> = conv
        .results
        .iter()
        .map(|r| serde_json::Value::String(r.rel_path.clone()))
        .collect();
    files.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    let levels: Vec<serde_json::Value> = {
        let mut v: Vec<u32> = conv.results.iter().map(|r| r.level).collect();
        v.sort_unstable();
        v.dedup();
        v.into_iter().map(serde_json::Value::from).collect()
    };
    let manifest = serde_json::json!({
        "version": ab_version,
        "sceneId": conv.scene_id,
        "levels": levels,
        "files": files,
        "exitCode": if conv.results.is_empty() { 1 } else { 0 },
    });
    let text = serde_json::to_string_pretty(&manifest)?;
    let mpath = entity_dir.join("LOD.manifest.json");
    std::fs::write(&mpath, &text)?;
    write_brotli_sidecar(&mpath, text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lod_filenames() {
        let (sid, lvl, ext) =
            parse_lod_filename("https://b.s3.amazonaws.com/-17,-21/LOD/Sources/170/bafkrei_0.fbx")
                .unwrap();
        assert_eq!(sid, "bafkrei");
        assert_eq!(lvl, 0);
        assert_eq!(ext, ".fbx");

        let (sid, lvl, ext) = parse_lod_filename("BafkReiABC_2.glb").unwrap();
        assert_eq!(sid, "bafkreiabc");
        assert_eq!(lvl, 2);
        assert_eq!(ext, ".glb");

        let (sid, lvl, _) = parse_lod_filename("x/scene_1.glb?token=abc").unwrap();
        assert_eq!(sid, "scene");
        assert_eq!(lvl, 1);
    }

    #[test]
    fn rejects_bad_names() {
        assert!(parse_lod_filename("noLevel.glb").is_err());
        assert!(parse_lod_filename("scene_x.glb").is_err());
        assert!(parse_lod_filename("_3.glb").is_err());
    }

    #[test]
    fn bundle_name_matches_client_key() {
        assert_eq!(
            lod_bundle_name("BafkRei", 1, "windows"),
            "bafkrei_1_windows"
        );
        assert_eq!(lod_bundle_name("scene", 0, "mac"), "scene_0_mac");
    }

    #[test]
    fn rel_path_is_per_level_folder() {
        assert_eq!(lod_rel_path(2, "scene_2_windows"), "LOD/2/scene_2_windows");
    }

    #[test]
    fn s3_urls_built() {
        let urls = s3_source_urls(
            DEFAULT_LODS_BUCKET,
            "-17,-21",
            "1707776785658",
            "bafkrei",
            &[0, 1, 2],
            ".fbx",
        );
        assert_eq!(urls.len(), 3);
        assert!(urls[0].ends_with("/-17,-21/LOD/Sources/1707776785658/bafkrei_0.fbx"));
        assert!(urls[2].ends_with("bafkrei_2.fbx"));
    }
}
