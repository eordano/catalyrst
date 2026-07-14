use super::*;

pub(crate) fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

pub(crate) fn load_env_file(path: &str) {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if std::env::var(key).is_err() {
                std::env::set_var(key, value);
            }
        }
    }
}

/// Do two configured storage roots name the same tree?
///
/// Resolved, not compared as strings: the answer decides whether a SECOND `ContentStorage` is built
/// over a tree that already has one, and `./content`, `/var/lib/catalyrst/content` and a symlink
/// between them are all the same files while looking nothing alike. A root that does not exist yet
/// cannot be resolved, so that case falls back to the literal comparison -- which still catches the
/// ordinary way this happens, two env vars set to the same value.
pub(crate) fn same_storage_root(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

pub(crate) struct LiveContentStorage {
    /// Shared, never one-per-consumer: `ContentStorage` carries the per-instance record of which
    /// shard directories it has created or observed, and that is what tells a destroyed shard from
    /// one that never existed. Two instances over the same root disagree about the same damage --
    /// the writer faults, the read-heavy HTTP instance (which created nothing) reports 404 -- so a
    /// root gets exactly one instance and everyone clones the `Arc`.
    pub(crate) inner: Arc<catalyrst_storage::ContentStorage>,
}

fn miss_on_invalid_id<T>(
    res: Result<Option<T>, catalyrst_storage::StorageError>,
) -> Result<Option<T>, catalyrst_storage::StorageError> {
    use catalyrst_storage::StorageError;
    match res {
        Err(StorageError::InvalidId(_)) | Err(StorageError::PathTraversal(_)) => Ok(None),
        other => other,
    }
}

#[async_trait]
impl ContentStorage for LiveContentStorage {
    async fn retrieve(&self, hash: &str) -> Result<Option<Bytes>, catalyrst_storage::StorageError> {
        miss_on_invalid_id(self.inner.retrieve(hash).await)
    }

    async fn retrieve_stream(
        &self,
        hash: &str,
    ) -> Result<Option<(Body, u64)>, catalyrst_storage::StorageError> {
        // One open, one decision. Statting via `file_path()` and then opening meant a second chance
        // to be told the file was gone, and absorbing THAT `ENOENT` as `Ok(None)` broke the trait's
        // contract (state.rs): a shard destroyed between the two syscalls became a 404 instead of a
        // fault. `open_for_read` decides absence once, from the descriptor it hands back.
        let Some((file, size)) = miss_on_invalid_id(self.inner.open_for_read(hash).await)? else {
            return Ok(None);
        };
        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);
        Ok(Some((body, size)))
    }

    /// The window only. Reading the file and slicing it made a 32-byte mime sniff of a 30 MB model
    /// cost 30 MB of resident bytes, once per concurrent request; `read_range` seeks the descriptor
    /// it opened, so the cost is the range the caller asked for.
    async fn retrieve_range(
        &self,
        hash: &str,
        start: u64,
        end: u64,
    ) -> Result<Option<Bytes>, catalyrst_storage::StorageError> {
        miss_on_invalid_id(self.inner.read_range(hash, start, end).await)
    }

    async fn file_info(
        &self,
        hash: &str,
    ) -> Result<Option<FileInfo>, catalyrst_storage::StorageError> {
        let info = miss_on_invalid_id(self.inner.file_info(hash).await)?;
        Ok(info.map(|info| FileInfo {
            size: Some(info.size),
            content_size: info.content_size,
            encoding: info.encoding,
        }))
    }

    async fn exist_multiple(
        &self,
        hashes: &[String],
    ) -> Result<HashMap<String, bool>, catalyrst_storage::StorageError> {
        let refs: Vec<&str> = hashes.iter().map(|s| s.as_str()).collect();
        let results = self.inner.exist_multiple(&refs).await?;
        Ok(results.into_iter().collect())
    }
}
