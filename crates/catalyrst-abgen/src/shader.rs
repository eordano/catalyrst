use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const SHADER_CAB: &str = "CAB-51fbd4c9d0fb3e603fd599ac9f5d01e1";

pub const SHADER_PATH_ID: i64 = 7_645_288_030_342_540_701;

pub const SHADER_FILE_ID: i64 = 1;

pub const SHADER_NAME: &str = "DCL/Scene";

pub const TEXARRAY_SHADER_NAME: &str = "DCL/Scene_TexArray";

pub const TEXARRAY_SHADER_PATH_ID: i64 = 2_346_303_084_350_958_154;

pub fn texarray_bundle_name(target: &str) -> String {
    format!("dcl/scene_texarray_ignore_{target}")
}

pub const VENDORED_FILE: &str = "scene_ignore_windows";

pub const VENDORED_SHA256: &str =
    "5a5ce6694c85b77be165e367fc510f2c8f06a05fa1422330fcff4c3793d6c4b5";

pub fn vendored_path() -> PathBuf {
    if let Ok(p) = std::env::var("ABGEN_SHADER_BUNDLE") {
        return PathBuf::from(p);
    }
    let compiled = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("shader")
        .join(VENDORED_FILE);
    if compiled.is_file() {
        return compiled;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join("shader").join(VENDORED_FILE);
            if bundled.is_file() {
                return bundled;
            }
        }
    }
    compiled
}

pub fn bundle_bytes() -> Result<Vec<u8>> {
    let p = vendored_path();
    std::fs::read(&p).with_context(|| {
        format!(
            "vendored shader bundle missing at {} — set ABGEN_SHADER_BUNDLE or \
             restore shader/{VENDORED_FILE} (CAB {SHADER_CAB})",
            p.display()
        )
    })
}

fn sha256_hex(data: &[u8]) -> String {
    crate::hashes::sha256_hex(data)
}

pub fn bundle_bytes_verified() -> Result<Vec<u8>> {
    let data = bundle_bytes()?;
    let got = sha256_hex(&data);
    if got != VENDORED_SHA256 {
        anyhow::bail!(
            "vendored shader bundle sha256 mismatch: got {got}, expected {VENDORED_SHA256}"
        );
    }
    Ok(data)
}

#[derive(Debug, Clone)]
pub struct Emitted {
    pub path: PathBuf,

    pub file_name: String,

    pub version: Option<String>,
}

pub fn emit(out_dir: &Path, version: Option<&str>, verify: bool) -> Result<Emitted> {
    let data = if verify {
        bundle_bytes_verified()?
    } else {
        bundle_bytes()?
    };
    let dir = match version {
        Some(v) => out_dir.join(v),
        None => out_dir.to_path_buf(),
    };
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating shader output dir {}", dir.display()))?;
    let path = dir.join(VENDORED_FILE);
    std::fs::write(&path, &data)
        .with_context(|| format!("writing shader bundle to {}", path.display()))?;
    Ok(Emitted {
        path,
        file_name: VENDORED_FILE.to_string(),
        version: version.map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texarray_bundle_names_hash_to_production_cabs() {
        assert_eq!(
            texarray_bundle_name("windows"),
            "dcl/scene_texarray_ignore_windows"
        );
        assert_eq!(
            crate::cabname::cab_name(&texarray_bundle_name("windows")),
            "CAB-bcf033c2b28284331ae8753f2af5e039"
        );
        assert_eq!(
            crate::cabname::cab_name(&texarray_bundle_name("mac")),
            "CAB-2f95afafeab990fc349e5ab530941444"
        );
    }

    #[test]
    fn vendored_bundle_matches_identity() {
        let p = vendored_path();
        if !p.exists() {
            eprintln!("vendored shader bundle missing, skipping: {}", p.display());
            return;
        }
        let data = bundle_bytes_verified().expect("verified read");
        assert_eq!(sha256_hex(&data), VENDORED_SHA256);

        assert!(data.starts_with(b"UnityFS"), "not a UnityFS bundle");

        let needle = SHADER_CAB.as_bytes();
        assert!(
            data.windows(needle.len()).any(|w| w == needle),
            "expected CAB {SHADER_CAB} in the bundle directory"
        );
    }

    #[test]
    fn emit_writes_versioned_layout() {
        let p = vendored_path();
        if !p.exists() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("abgen_shader_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let e = emit(&tmp, Some("v0-abgen"), true).expect("emit");
        assert!(e.path.exists());
        assert_eq!(e.file_name, VENDORED_FILE);
        assert_eq!(e.version.as_deref(), Some("v0-abgen"));
        assert!(e.path.ends_with(format!("v0-abgen/{VENDORED_FILE}")));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
