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

pub(crate) struct LiveContentStorage {
    pub(crate) inner: catalyrst_storage::ContentStorage,
}

#[async_trait]
impl ContentStorage for LiveContentStorage {
    async fn retrieve(&self, hash: &str) -> Option<Bytes> {
        self.inner.retrieve(hash).await.ok().flatten()
    }

    async fn retrieve_stream(&self, hash: &str) -> Option<(Body, u64)> {
        let (path, _is_gzip) = self.inner.file_path(hash).await.ok()??;
        let file = tokio::fs::File::open(&path).await.ok()?;
        let metadata = file.metadata().await.ok()?;
        let size = metadata.len();
        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);
        Some((body, size))
    }

    async fn retrieve_range(&self, hash: &str, start: u64, end: u64) -> Option<Bytes> {
        let data = self
            .inner
            .retrieve_uncompressed(hash)
            .await
            .ok()
            .flatten()?;
        let start = start as usize;
        let end = (end as usize).min(data.len().saturating_sub(1));
        if start > end || start >= data.len() {
            return None;
        }
        Some(data.slice(start..=end))
    }

    async fn file_info(&self, hash: &str) -> Option<FileInfo> {
        let info = self.inner.file_info(hash).await.ok()??;
        Some(FileInfo {
            size: Some(info.size),
            content_size: info.content_size,
            encoding: info.encoding,
        })
    }

    async fn exist_multiple(&self, hashes: &[String]) -> HashMap<String, bool> {
        let refs: Vec<&str> = hashes.iter().map(|s| s.as_str()).collect();
        match self.inner.exist_multiple(&refs).await {
            Ok(results) => results.into_iter().collect(),
            Err(_) => hashes.iter().map(|h| (h.clone(), false)).collect(),
        }
    }
}
