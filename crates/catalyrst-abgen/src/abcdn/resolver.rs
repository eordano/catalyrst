use std::path::{Path, PathBuf};

pub const PLATFORMS: &[(&str, &str)] = &[
    ("_windows", "windows"),
    ("_mac", "mac"),
    ("_linux", "linux"),
    ("_webgl", "webgl"),
];

pub fn is_platform(name: &str) -> bool {
    PLATFORMS.iter().any(|(_, p)| *p == name)
}

pub fn platform_of(name: &str) -> &'static str {
    split_platform(name).0
}

pub fn split_platform(name: &str) -> (&'static str, &str) {
    for (suffix, bare) in PLATFORMS {
        if let Some(stem) = name.strip_suffix(suffix) {
            return (bare, stem);
        }
    }
    ("webgl", name)
}

pub fn is_safe_component(c: &str) -> bool {
    !c.is_empty()
        && c != "."
        && c != ".."
        && !c.contains('/')
        && !c.contains('\\')
        && !c.contains('\0')
}

pub fn manifest_path(root: &Path, name_with_suffix: &str) -> Option<PathBuf> {
    let (platform, entity_id) = split_platform(name_with_suffix);
    if !is_safe_component(entity_id) {
        return None;
    }
    Some(
        root.join(entity_id)
            .join(format!("{platform}.manifest.json")),
    )
}

pub fn binary_path(root: &Path, entity: &str, filename: &str) -> Option<PathBuf> {
    if !is_safe_component(entity) || !is_safe_component(filename) {
        return None;
    }
    let flat = root.join(filename);
    if flat.is_file() {
        return Some(flat);
    }
    let name_for_platform = filename.strip_suffix(".br").unwrap_or(filename);
    let platform = platform_of(name_for_platform);
    Some(root.join(entity).join(platform).join(filename))
}

pub fn lod_path(root: &Path, level: &str, filename: &str) -> Option<PathBuf> {
    if !is_safe_component(level) || !is_safe_component(filename) {
        return None;
    }
    let raw = filename.strip_suffix(".br").unwrap_or(filename);
    let (_, no_platform) = split_platform(raw);
    let scene_id = no_platform
        .strip_suffix(&format!("_{level}"))
        .unwrap_or(no_platform);
    if !is_safe_component(scene_id) {
        return None;
    }
    Some(root.join(scene_id).join("LOD").join(level).join(filename))
}

pub fn iss_manifest_path(root: &Path, filename: &str) -> Option<PathBuf> {
    if !is_safe_component(filename) {
        return None;
    }
    let stem = filename.strip_suffix(".br").unwrap_or(filename);
    let sid = stem.strip_suffix(crate::lodgen::placements::ISS_SUFFIX)?;
    if sid.is_empty() || !is_safe_component(sid) {
        return None;
    }
    Some(root.join(sid).join(filename))
}

pub const SHADER_PLATFORMS: [&str; 3] = ["windows", "mac", "linux"];

pub struct ShaderTarget {
    pub url_ver: String,
    pub canonical: String,
}

pub fn shader_allowlisted(canonical: &str) -> bool {
    SHADER_PLATFORMS.iter().any(|p| {
        canonical == format!("dcl/scene_ignore_{p}")
            || canonical == format!("dcl/universal render pipeline/lit_ignore_{p}")
            || canonical == crate::shader::texarray_bundle_name(p)
    })
}

fn hexval(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(hi), Some(lo)) = (hexval(b[i + 1]), hexval(b[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn shader_target(path: &str) -> Option<ShaderTarget> {
    let decoded = percent_decode(path);
    let segs: Vec<&str> = decoded.split('/').collect();
    if segs.len() < 3 || !is_safe_component(segs[0]) {
        return None;
    }
    if matches!(segs[0], "manifest" | "LOD" | "lods-unity") {
        return None;
    }
    for seg in &segs[1..] {
        if !is_safe_component(seg) {
            return None;
        }
    }
    let canonical = if segs[1] == "dcl" {
        segs[1..].join("/")
    } else if segs.len() >= 4 && segs[2] == "dcl" {
        segs[2..].join("/")
    } else {
        return None;
    };
    if !shader_allowlisted(&canonical) {
        return None;
    }
    Some(ShaderTarget {
        url_ver: segs[0].to_string(),
        canonical,
    })
}

pub fn shader_path(root: &Path, canonical: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    for seg in canonical.split('/') {
        if !is_safe_component(seg) {
            return None;
        }
        out.push(seg);
    }
    Some(out)
}

pub fn resolve_with_casing(exact: &Path) -> Option<PathBuf> {
    if exact.is_file() {
        return Some(exact.to_path_buf());
    }
    let parent = exact.parent()?;
    let target = exact.file_name()?.to_str()?.to_ascii_lowercase();
    let mut found: Option<PathBuf> = None;
    for entry in std::fs::read_dir(parent).ok()? {
        let entry = entry.ok()?;
        if let Some(name) = entry.file_name().to_str() {
            if name.to_ascii_lowercase() == target && entry.path().is_file() {
                if found.is_some() {
                    return None;
                }
                found = Some(entry.path());
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn platform_detection() {
        assert_eq!(platform_of("Qm123_windows"), "windows");
        assert_eq!(platform_of("bafk_mac"), "mac");
        assert_eq!(platform_of("x_linux"), "linux");
        assert_eq!(platform_of("Qm123"), "webgl");
        assert_eq!(platform_of("Qm123_webgl"), "webgl");
        assert_eq!(split_platform("Qm123_webgl"), ("webgl", "Qm123"));
        assert_eq!(platform_of("staticscene_3_mac"), "mac");
    }

    #[test]
    fn manifest_mapping() {
        let root = Path::new("/out");
        assert_eq!(
            manifest_path(root, "bafkEnt_windows").unwrap(),
            Path::new("/out/bafkEnt/windows.manifest.json")
        );
        assert_eq!(
            manifest_path(root, "bafkEnt").unwrap(),
            Path::new("/out/bafkEnt/webgl.manifest.json")
        );
    }

    #[test]
    fn binary_mapping() {
        let root = Path::new("/out");
        assert_eq!(
            binary_path(root, "bafkScene", "Qmhash_windows").unwrap(),
            Path::new("/out/bafkScene/windows/Qmhash_windows")
        );
        assert_eq!(
            binary_path(root, "bafkScene", "Qmhash_mac.br").unwrap(),
            Path::new("/out/bafkScene/mac/Qmhash_mac.br")
        );
    }

    #[test]
    fn lod_mapping() {
        let root = Path::new("/out");
        assert_eq!(
            lod_path(root, "1", "bafkscene_1_mac").unwrap(),
            Path::new("/out/bafkscene/LOD/1/bafkscene_1_mac")
        );
        assert_eq!(
            lod_path(root, "2", "bafkscene_2_windows.br").unwrap(),
            Path::new("/out/bafkscene/LOD/2/bafkscene_2_windows.br")
        );
        assert_eq!(
            lod_path(root, "0", "bafkscene_0").unwrap(),
            Path::new("/out/bafkscene/LOD/0/bafkscene_0")
        );
    }

    #[test]
    fn iss_manifest_mapping() {
        let root = Path::new("/out");
        assert_eq!(
            iss_manifest_path(root, "bafkscene_InitialSceneState.json").unwrap(),
            Path::new("/out/bafkscene/bafkscene_InitialSceneState.json")
        );
        assert_eq!(
            iss_manifest_path(root, "bafkscene_InitialSceneState.json.br").unwrap(),
            Path::new("/out/bafkscene/bafkscene_InitialSceneState.json.br")
        );
        assert!(iss_manifest_path(root, "bafkscene-lod-manifest.json").is_none());
        assert!(iss_manifest_path(root, "bafkscene_InitialSceneState.jsonx").is_none());
        assert!(iss_manifest_path(root, "LOD.manifest.json").is_none());
        assert!(iss_manifest_path(root, "_InitialSceneState.json").is_none());
        assert!(iss_manifest_path(root, "_InitialSceneState.json.br").is_none());
        assert!(iss_manifest_path(root, ".._InitialSceneState.json").is_none());
        assert!(iss_manifest_path(root, "a/b_InitialSceneState.json").is_none());
        assert!(iss_manifest_path(root, "a\\b_InitialSceneState.json").is_none());
    }

    #[test]
    fn rejects_traversal() {
        assert!(!is_safe_component("../etc"));
        assert!(!is_safe_component("a/b"));
        assert!(manifest_path(Path::new("/out"), "../../etc/passwd").is_none());
        assert!(binary_path(Path::new("/out"), "..", "x").is_none());
    }

    #[test]
    fn shader_target_strips_scene_id_to_one_canonical() {
        let three = shader_target("v41/dcl/scene_ignore_windows").unwrap();
        assert_eq!(three.url_ver, "v41");
        assert_eq!(three.canonical, "dcl/scene_ignore_windows");

        let four = shader_target("v41/bafkscene/dcl/scene_ignore_windows").unwrap();
        assert_eq!(four.url_ver, "v41");
        assert_eq!(four.canonical, "dcl/scene_ignore_windows");

        let lit4 = shader_target("v41/dcl/universal render pipeline/lit_ignore_mac").unwrap();
        assert_eq!(
            lit4.canonical,
            "dcl/universal render pipeline/lit_ignore_mac"
        );

        let lit5 =
            shader_target("v41/bafkscene/dcl/universal render pipeline/lit_ignore_mac").unwrap();
        assert_eq!(lit5.canonical, lit4.canonical);

        let tex = shader_target("v41/bafkscene/dcl/scene_texarray_ignore_linux").unwrap();
        assert_eq!(tex.canonical, "dcl/scene_texarray_ignore_linux");

        let enc = shader_target("v41/dcl/universal%20render%20pipeline/lit_ignore_mac").unwrap();
        assert_eq!(
            enc.canonical,
            "dcl/universal render pipeline/lit_ignore_mac"
        );
        let enc5 = shader_target("v41/bafkscene/dcl/universal%20render%20pipeline/lit_ignore_mac")
            .unwrap();
        assert_eq!(enc5.canonical, enc.canonical);
    }

    #[test]
    fn shader_target_rejects_non_allowlisted_and_traversal() {
        assert!(shader_target("v41/dcl/scene_ignore_webgl").is_none());
        assert!(shader_target("v41/dcl/anything_else").is_none());
        assert!(shader_target("v41/bafkscene/notdcl/scene_ignore_windows").is_none());
        assert!(shader_target("v41/../dcl/scene_ignore_windows").is_none());
        assert!(shader_target("v41/dcl/../scene_ignore_windows").is_none());
        assert!(shader_target("v41/bafkEntity/some/nested/file.bin").is_none());
        assert!(shader_target("v41/dcl").is_none());
        assert!(shader_target("dcl/scene_ignore_windows").is_none());
        assert!(shader_target("manifest/dcl/scene_ignore_windows").is_none());
        assert!(shader_target("LOD/dcl/scene_ignore_windows").is_none());
        assert!(shader_target("v41/dcl/scene_ignore_windows.br").is_none());
        assert!(shader_allowlisted(
            "dcl/universal render pipeline/lit_ignore_windows"
        ));
        assert!(!shader_allowlisted(
            "dcl/universal render pipeline/lit_ignore_webgl"
        ));
    }

    #[test]
    fn shader_disk_path_nests_under_root() {
        assert_eq!(
            shader_path(Path::new("/out"), "dcl/scene_ignore_windows").unwrap(),
            Path::new("/out/dcl/scene_ignore_windows")
        );
        assert_eq!(
            shader_path(
                Path::new("/out"),
                "dcl/universal render pipeline/lit_ignore_mac"
            )
            .unwrap(),
            Path::new("/out/dcl/universal render pipeline/lit_ignore_mac")
        );
        assert!(shader_path(Path::new("/out"), "dcl/../x").is_none());
    }
}
