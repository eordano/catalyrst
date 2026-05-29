mod content_storage;
mod snapshot_storage;

pub use content_storage::ContentStorage;
pub use snapshot_storage::SnapshotStorage;

use sha1::{Digest, Sha1};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("path traversal detected: file id {0:?} would escape the storage root")]
    PathTraversal(String),

    #[error("invalid content id {0:?}: must be a canonical CIDv0 or CIDv1")]
    InvalidId(String),
}

pub fn hex_prefix(id: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(id.as_bytes());
    let digest = hasher.finalize();
    format!("{:02x}{:02x}", digest[0], digest[1])
}

pub(crate) fn is_canonical_content_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 100 {
        return false;
    }

    let cidv0 = id.len() == 46
        && id.starts_with("Qm")
        && id[2..].chars().all(|c| {
            matches!(c,
                '1'..='9' | 'A'..='H' | 'J'..='N' | 'P'..='Z' | 'a'..='k' | 'm'..='z')
        });

    let cidv1 = id.starts_with("ba")
        && id.len() >= 58
        && id[2..].chars().all(|c| matches!(c, 'a'..='z' | '2'..='7'));
    cidv0 || cidv1
}

async fn resolve_file_path(root: &PathBuf, id: &str) -> Result<PathBuf, StorageError> {
    if !is_canonical_content_id(id) {
        return Err(StorageError::InvalidId(id.to_string()));
    }

    let prefix = hex_prefix(id);
    let dir = root.join(&prefix);
    let file_path = dir.join(id);

    let normalized = file_path
        .components()
        .fold(PathBuf::new(), |mut acc, c| {
            match c {
                std::path::Component::ParentDir => {
                    acc.pop();
                }
                other => acc.push(other),
            }
            acc
        });

    if !normalized.starts_with(root) {
        return Err(StorageError::PathTraversal(id.to_owned()));
    }

    tokio::fs::create_dir_all(&dir).await?;

    Ok(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_prefix_matches_typescript() {
        assert_eq!(
            hex_prefix("QmcoQSrVoi8CKSwiRyJ3MPYyN1AUiLjHiAtYCUGoBr8JM4"),
            "f049"
        );
        assert_eq!(
            hex_prefix("bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa"),
            "f049"
        );
    }

    #[test]
    fn path_traversal_is_blocked() {
        let root = PathBuf::from("/tmp/storage");
        let id = "../../../etc/passwd";
        let prefix = hex_prefix(id);
        let dir = root.join(&prefix);
        let file_path = dir.join(id);
        let normalized = file_path
            .components()
            .fold(PathBuf::new(), |mut acc, c| {
                match c {
                    std::path::Component::ParentDir => {
                        acc.pop();
                    }
                    other => acc.push(other),
                }
                acc
            });
        assert!(!normalized.starts_with(&root));
    }

    #[test]
    fn canonical_id_accepts_valid_cidv0() {

        assert!(is_canonical_content_id(
            "QmcoQSrVoi8CKSwiRyJ3MPYyN1AUiLjHiAtYCUGoBr8JM4"
        ));
        assert!(is_canonical_content_id(
            "QmaozNR7DZHQK1ZcU9p7QdrshMvXqWK6gpu5rmrkPdT3L4"
        ));
    }

    #[test]
    fn canonical_id_accepts_valid_cidv1() {
        assert!(is_canonical_content_id(
            "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa"
        ));
        assert!(is_canonical_content_id(
            "bafkreifzjut3te2nhyekklss27nh3k72ysco7y32koao5eei66wof36n5e"
        ));
    }

    #[test]
    fn canonical_id_rejects_path_separator() {
        assert!(!is_canonical_content_id("foo/bar"));
    }

    #[test]
    fn canonical_id_rejects_parent_dir() {
        assert!(!is_canonical_content_id("../etc/passwd"));
    }

    #[test]
    fn canonical_id_rejects_nul_byte() {
        assert!(!is_canonical_content_id("Qm\0evil"));
    }

    #[test]
    fn canonical_id_rejects_invalid_base58_chars() {

        let bad = format!("Qm{}", "0".repeat(44));
        assert!(!is_canonical_content_id(&bad));
    }

    #[test]
    fn canonical_id_rejects_empty() {
        assert!(!is_canonical_content_id(""));
    }

    #[test]
    fn canonical_id_rejects_too_long() {
        let long = format!("ba{}", "a".repeat(200));
        assert!(!is_canonical_content_id(&long));
    }

    #[test]
    fn canonical_id_rejects_backslash() {
        assert!(!is_canonical_content_id("foo\\bar"));
    }

    #[test]
    fn canonical_id_rejects_cidv0_wrong_length() {

        assert!(!is_canonical_content_id("QmcoQSrVoi8C"));
    }

    #[test]
    fn canonical_id_rejects_cidv1_uppercase() {

        assert!(!is_canonical_content_id(
            "BAFKREIE4EISVKZYJUQRCENDYDK6VIKQS2VCO5LMIB4NLZSXTJZOFIQY2PA"
        ));
    }
}
