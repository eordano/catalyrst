use anyhow::{Context, Result};
use memmap2::Mmap;
use std::fs::File;
use std::path::{Path, PathBuf};

pub fn mmap_file(path: &Path) -> Result<Mmap> {
    let f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    unsafe { Mmap::map(&f) }.with_context(|| format!("mmap {}", path.display()))
}

pub const ABGEN_CONTENT_ROOT_ENV: &str = "ABGEN_CONTENT_ROOT";
pub const DEFAULT_CONTENT_ROOT: &str = "./content";

pub struct LocalContentStore {
    root: PathBuf,
}

impl LocalContentStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_for(&self, cid: &str) -> PathBuf {
        use sha1::{Digest, Sha1};
        let digest = Sha1::digest(cid.as_bytes());
        let mut prefix = String::with_capacity(4);
        for b in &digest[..2] {
            prefix.push(char::from_digit((b >> 4) as u32, 16).unwrap());
            prefix.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
        }
        self.root.join(prefix).join(cid)
    }

    pub fn fetch(&self, cid: &str) -> Result<Vec<u8>> {
        let path = self.path_for(cid);
        let mm = mmap_file(&path)
            .with_context(|| format!("local content store: {} (CID {cid})", path.display(),))?;
        Ok(mm.to_vec())
    }

    pub fn fetch_mmap(&self, cid: &str) -> Result<Mmap> {
        let path = self.path_for(cid);
        mmap_file(&path)
            .with_context(|| format!("local content store: {} (CID {cid})", path.display(),))
    }

    pub fn exists(&self, cid: &str) -> bool {
        self.path_for(cid).exists()
    }

    pub fn write(&self, cid: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path_for(cid);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("create shard dir {}", dir.display()))?;
        }
        let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("rename into {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_for_uses_sha1_first_four_hex() {
        let s = LocalContentStore::new("/dev/null/x");
        let p = s.path_for("bafkreibxefote3jeusciwqxxrvwu5b4qi7uzg6lf3avexadfg7xkkz5gge");
        let prefix = p.parent().unwrap().file_name().unwrap().to_string_lossy();
        assert_eq!(prefix, "91f7");
    }

    #[test]
    fn fetch_missing_returns_clear_error() {
        let s = LocalContentStore::new("/nonexistent/abgen-local-store");
        let err = s.fetch("bafkrei000000").unwrap_err().to_string();
        assert!(err.contains("local content store"), "{err}");
    }
}
