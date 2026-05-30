use std::path::{Path, PathBuf};

const PLATFORMS: &[(&str, &str)] = &[
    ("_windows", "windows"),
    ("_mac", "mac"),
    ("_linux", "linux"),
];

pub fn platform_of(name: &str) -> &'static str {
    for (suffix, bare) in PLATFORMS {
        if name.ends_with(suffix) {
            return bare;
        }
    }
    "webgl"
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
    let platform = platform_of(name_with_suffix);
    let entity_id = match PLATFORMS.iter().find(|(s, _)| name_with_suffix.ends_with(s)) {
        Some((suffix, _)) => &name_with_suffix[..name_with_suffix.len() - suffix.len()],
        None => name_with_suffix,
    };
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
    let no_platform = match PLATFORMS.iter().find(|(s, _)| raw.ends_with(s)) {
        Some((suffix, _)) => &raw[..raw.len() - suffix.len()],
        None => raw,
    };
    let scene_id = no_platform
        .strip_suffix(&format!("_{level}"))
        .unwrap_or(no_platform);
    if !is_safe_component(scene_id) {
        return None;
    }
    Some(root.join(scene_id).join("LOD").join(level).join(filename))
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
    fn rejects_traversal() {
        assert!(!is_safe_component("../etc"));
        assert!(!is_safe_component("a/b"));
        assert!(manifest_path(Path::new("/out"), "../../etc/passwd").is_none());
        assert!(binary_path(Path::new("/out"), "..", "x").is_none());
    }
}
