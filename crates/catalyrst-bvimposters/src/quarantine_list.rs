use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::key::{parse_zip_request, ImposterKey};
use crate::store::Store;

pub struct QuarantineList {
    path: PathBuf,
    keys: HashSet<ImposterKey>,
}

#[derive(Default, Debug)]
pub struct ApplyCounts {
    pub renamed: usize,
    pub absent: usize,
    pub errors: usize,
}

pub fn parse_line(line: &str) -> Option<ImposterKey> {
    let (level, rest) = line.split_once('/')?;
    let file = if rest.ends_with(".zip") {
        rest.to_string()
    } else {
        format!("{rest}.zip")
    };
    parse_zip_request(level, &file)
}

impl QuarantineList {
    pub fn load(path: PathBuf) -> Self {
        let mut keys = HashSet::new();
        let mut invalid = 0usize;
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                match parse_line(trimmed) {
                    Some(key) => {
                        keys.insert(key);
                    }
                    None => invalid += 1,
                }
            }
        }
        if invalid > 0 {
            tracing::warn!(path = %path.display(), invalid, "quarantine list has unparseable lines");
        }
        Self { path, keys }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn contains(&self, key: &ImposterKey) -> bool {
        self.keys.contains(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &ImposterKey> {
        self.keys.iter()
    }
}

pub fn apply(store: &Store, list: &QuarantineList) -> ApplyCounts {
    let mut counts = ApplyCounts::default();
    let mut keys: Vec<_> = list.keys().collect();
    keys.sort_by_key(|key| (key.tile.level, key.tile.x, key.tile.y, key.crc));
    for key in keys {
        match store.quarantine_entry(key) {
            Ok(true) => counts.renamed += 1,
            Ok(false) => counts.absent += 1,
            Err(e) => {
                tracing::warn!(
                    key = %format!("{}/{}", key.tile.level, key.zip_name()),
                    error = %e,
                    "quarantine rename failed"
                );
                counts.errors += 1;
            }
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines_with_and_without_zip_suffix() {
        let key = parse_line("0/0,100.3504527830.zip").unwrap();
        assert_eq!((key.tile.level, key.tile.x, key.tile.y), (0, 0, 100));
        assert_eq!(key.crc, 3504527830);
        assert_eq!(parse_line("2/-64,-128.7").unwrap().crc, 7);
    }

    #[test]
    fn rejects_invalid_lines() {
        assert!(parse_line("6/0,0.1.zip").is_none());
        assert!(parse_line("0/0,100.0.zip").is_none());
        assert!(parse_line("1/1,0.5.zip").is_none());
        assert!(parse_line("nonsense").is_none());
    }

    #[test]
    fn loads_file_skipping_comments_and_blanks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("list.txt");
        std::fs::write(
            &path,
            "# header\n\n0/0,100.3504527830.zip\n1/-2,4.9\nbad line\n0/0,100.3504527830\n",
        )
        .unwrap();
        let list = QuarantineList::load(path);
        assert_eq!(list.len(), 2);
        let key = ImposterKey::new(0, 0, 100, 3504527830).unwrap();
        assert!(list.contains(&key));
        assert!(list.contains(&ImposterKey::new(1, -2, 4, 9).unwrap()));
        assert!(!list.contains(&ImposterKey::new(0, 0, 100, 1).unwrap()));
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let list = QuarantineList::load(dir.path().join("nope.txt"));
        assert!(list.is_empty());
    }

    #[test]
    fn apply_renames_present_and_counts_absent() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("root"), u64::MAX);
        store.init().unwrap();
        let present = ImposterKey::new(0, 0, 100, 42).unwrap();
        std::fs::create_dir_all(store.level_dir(0)).unwrap();
        std::fs::write(store.zip_path(&present), b"zipbytes").unwrap();
        let path = dir.path().join("list.txt");
        std::fs::write(&path, "0/0,100.42.zip\n3/8,-16.7.zip\n").unwrap();
        let list = QuarantineList::load(path);
        let counts = apply(&store, &list);
        assert_eq!(counts.renamed, 1);
        assert_eq!(counts.absent, 1);
        assert_eq!(counts.errors, 0);
        assert!(!store.zip_path(&present).exists());
        assert!(store
            .quarantined_dir()
            .join("0")
            .join("0,100.42.zip")
            .exists());
    }
}
