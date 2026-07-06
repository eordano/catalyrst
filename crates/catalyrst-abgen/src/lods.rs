use crate::builder::{build_bundle, BuildOpts, LodBuildParams};
#[cfg(not(target_arch = "wasm32"))]
use crate::catalyst::CatalystClient;
#[cfg(not(target_arch = "wasm32"))]
use crate::compress;
use crate::naming;
use anyhow::{anyhow, bail, Context, Result};
#[cfg(not(target_arch = "wasm32"))]
use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

pub const DEFAULT_LODS_BUCKET: &str = "https://lods-bucket-ed4300a.s3.amazonaws.com";

#[derive(Clone, Debug)]
pub struct LodGenMeta {
    pub parcels: Vec<(i32, i32)>,
    pub base: (i32, i32),
    pub timestamp: Option<i64>,
    pub vertical_override: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct LodOptions {
    pub platform: String,

    pub ab_version: String,

    pub keep_forward_plus: bool,

    pub lod: Option<LodGenMeta>,
}

impl Default for LodOptions {
    fn default() -> Self {
        LodOptions {
            platform: "windows".to_string(),
            ab_version: crate::manifest::DEFAULT_AB_VERSION.to_string(),
            keep_forward_plus: true,
            lod: None,
        }
    }
}

pub fn plane_clipping(parcels: &[(i32, i32)]) -> [f64; 4] {
    let min_x = parcels.iter().map(|p| p.0).min().unwrap_or(0);
    let max_x = parcels.iter().map(|p| p.0).max().unwrap_or(0);
    let min_y = parcels.iter().map(|p| p.1).min().unwrap_or(0);
    let max_y = parcels.iter().map(|p| p.1).max().unwrap_or(0);
    [
        min_x as f64 * 16.0 - 0.05,
        (max_x + 1) as f64 * 16.0 + 0.05,
        min_y as f64 * 16.0 - 0.05,
        (max_y + 1) as f64 * 16.0 + 0.05,
    ]
}

pub fn vertical_clipping(n_parcels: usize) -> [f64; 4] {
    let height = 20.0f32 * crate::detmath::log2f((n_parcels + 1) as f32);
    [0.0, height as f64, 0.0, 0.0]
}

pub fn root_position(base: (i32, i32)) -> [f64; 3] {
    [base.0 as f64 * 16.0, 0.0, base.1 as f64 * 16.0]
}

pub fn lod_main_asset(scene_id: &str, level: u32) -> String {
    format!("{}_{}.prefab", scene_id.to_lowercase(), level)
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

pub fn validate_lod_platform(p: &str) -> Result<()> {
    match p {
        "windows" | "mac" | "linux" => Ok(()),
        "webgl" => bail!(
            "LOD platform \"webgl\" unsupported: upstream webgl LOD bundles use an empty \
             platform suffix and are not generated here (want windows|mac|linux)"
        ),
        other => bail!("unknown LOD platform {other:?} (want windows|mac|linux)"),
    }
}

pub fn lod_bundle_name(scene_id: &str, level: u32, platform: &str) -> String {
    format!("{}_{}_{}", scene_id.to_lowercase(), level, platform)
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn lod_rel_path(level: u32, bundle_name: &str) -> String {
    format!("LOD/{level}/{bundle_name}")
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn prepare_lod_source(client: &CatalystClient, locator: &str) -> Result<(String, u32, Vec<u8>)> {
    let (sid, level, ext) = parse_lod_filename(locator)?;
    if ext == ".fbx" {
        bail!(
            "FBX LOD source {locator:?}: FBX -> GLB transcoder not yet implemented \
             in the pure-Rust port (TODO: add an FBX importer crate or shell out to \
             a converter; track in the abgen issue tracker). Workaround: \
             re-export the asset as .glb upstream and re-run."
        );
    }
    if ext != ".glb" && ext != ".gltf" {
        bail!("unsupported LOD source extension {ext:?} for {locator:?}");
    }

    let glb = fetch_lod_bytes(client, locator)?;
    Ok((sid, level, glb))
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn build_lod_bundle(
    glb: &[u8],
    locator: &str,
    sid: &str,
    level: u32,
    platform: &str,
    opts: &LodOptions,
) -> Result<(LodResult, Vec<u8>)> {
    let bundle_name = lod_bundle_name(sid, level, platform);

    let root_hash = format!("{}_{}", sid, level);
    let lod_params: Option<LodBuildParams> = opts.lod.as_ref().map(|m| LodBuildParams {
        level,
        plane_clipping: plane_clipping(&m.parcels),
        vertical_clipping: match m.vertical_override {
            Some(h) => [0.0, h, 0.0, 0.0],
            None => vertical_clipping(m.parcels.len()),
        },
        root_position: root_position(m.base),
        main_asset: lod_main_asset(sid, level),
        timestamp: m.timestamp,
    });
    let build_opts = BuildOpts {
        keep_forward_plus: opts.keep_forward_plus,
        source_file: Some(locator),
        lod: lod_params.as_ref(),
        ..Default::default()
    };
    let data = build_bundle(glb, &bundle_name, &root_hash, &build_opts)
        .with_context(|| format!("build LOD bundle for {locator:?}"))?
        .data;

    let rel = lod_rel_path(level, &bundle_name);
    let result = LodResult {
        scene_id: sid.to_string(),
        level,
        bytes: data.len(),
        rel_path: rel,
        bundle_name,
    };
    Ok((result, data))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn convert_lods(
    client: &CatalystClient,
    sources: &[String],
    out_dir: &str,
    opts: &LodOptions,
) -> Result<LodConversion> {
    convert_lods_platforms(
        client,
        sources,
        out_dir,
        opts,
        std::slice::from_ref(&opts.platform),
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub fn convert_lods_platforms(
    client: &CatalystClient,
    sources: &[String],
    out_dir: &str,
    opts: &LodOptions,
    platforms: &[String],
) -> Result<LodConversion> {
    if sources.is_empty() {
        bail!("no LOD sources given");
    }
    let platform_list: Vec<String> = if platforms.is_empty() {
        vec![opts.platform.clone()]
    } else {
        platforms.to_vec()
    };
    let mut conv = LodConversion::default();

    let mut written: BTreeMap<String, (String, Vec<u8>)> = BTreeMap::new();
    let mut scene_id: Option<String> = None;

    for locator in sources {
        match prepare_lod_source(client, locator) {
            Ok((sid, level, glb)) => {
                for platform in &platform_list {
                    match build_lod_bundle(&glb, locator, &sid, level, platform, opts) {
                        Ok((r, data)) => {
                            scene_id.get_or_insert_with(|| r.scene_id.clone());
                            written.insert(r.bundle_name.clone(), (r.rel_path.clone(), data));
                            conv.results.push(r);
                        }
                        Err(e) => {
                            let key = if platform_list.len() == 1 {
                                locator.clone()
                            } else {
                                format!("{locator} [{platform}]")
                            };
                            conv.skipped.push((key, format!("{e:#}")));
                        }
                    }
                }
            }
            Err(e) => conv.skipped.push((locator.clone(), format!("{e:#}"))),
        }
    }

    let sid = match scene_id.clone() {
        Some(s) => s,
        None => {
            let mut msg = format!("no LOD source converted ({} skipped)", conv.skipped.len());
            for (locator, err) in &conv.skipped {
                msg.push_str(&format!("\n  {locator}: {err}"));
            }
            bail!("{msg}");
        }
    };
    conv.scene_id = sid.clone();

    let entity_dir = PathBuf::from(out_dir).join(&sid);
    for (rel, data) in written.values() {
        let path = entity_dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_atomic(&path, data)?;
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

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let mut tmp_os = path.as_os_str().to_owned();
    tmp_os.push(format!(".tmp.{}", std::process::id()));
    let tmp = PathBuf::from(tmp_os);
    std::fs::write(&tmp, data).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write_brotli_sidecar(path: &Path, data: &[u8]) -> Result<()> {
    let mut br = path.as_os_str().to_owned();
    br.push(".br");
    write_atomic(&PathBuf::from(br), &compress::brotli(data)?)?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
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
    write_atomic(&mpath, text.as_bytes())?;
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
    fn validate_lod_platform_matrix() {
        for ok in ["windows", "mac", "linux"] {
            assert!(validate_lod_platform(ok).is_ok(), "{ok}");
        }
        let webgl = format!("{:#}", validate_lod_platform("webgl").unwrap_err());
        assert!(webgl.contains("empty"), "{webgl}");
        assert!(webgl.contains("unsupported"), "{webgl}");
        for bad in ["", "osx", "win", "WINDOWS", "windows,mac"] {
            assert!(validate_lod_platform(bad).is_err(), "{bad:?}");
        }
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
    fn plane_clipping_is_world_parcel_rect_with_margin() {
        assert_eq!(
            plane_clipping(&[(8, -83)]),
            [127.95, 144.05, -1328.05, -1311.95]
        );
        let plaza: Vec<(i32, i32)> = (-3..=3)
            .flat_map(|x| (-4..=9).map(move |y| (x, y)))
            .collect();
        assert_eq!(plane_clipping(&plaza), [-48.05, 64.05, -64.05, 160.05]);
    }

    #[test]
    fn vertical_clipping_matches_height_limit_formula() {
        assert_eq!(vertical_clipping(1), [0.0, 20.0, 0.0, 0.0]);
        let v = vertical_clipping(70);
        assert!(
            (v[1] - 122.99493).abs() < 1e-3,
            "vertical_clipping(70)[1] = {}",
            v[1]
        );
        assert_eq!(v[0], 0.0);
        assert_eq!(v[2], 0.0);
        assert_eq!(v[3], 0.0);
    }

    #[test]
    fn root_position_is_base_parcel_world_origin() {
        assert_eq!(root_position((8, -83)), [128.0, 0.0, -1328.0]);
        assert_eq!(root_position((-3, -2)), [-48.0, 0.0, -32.0]);
    }

    #[test]
    fn main_asset_is_lowercased_prefab_key() {
        assert_eq!(lod_main_asset("BafkReiABC", 1), "bafkreiabc_1.prefab");
        assert_eq!(
            lod_main_asset("qmccggwqvb7v3b3vqxajzcjimmzhzrrvmk3ulkt6qxsesd", 1),
            "qmccggwqvb7v3b3vqxajzcjimmzhzrrvmk3ulkt6qxsesd_1.prefab"
        );
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
