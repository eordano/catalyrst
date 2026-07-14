use bytes::Bytes;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, warn};

use crate::{
    create_staging_file, ensure_file_path, open_for_read, resolve_file_path, staging_path,
    stat_for_read, FileIds, KnownShards, StorageError,
};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

pub struct ContentStorage {
    root: PathBuf,
    known_shards: KnownShards,
}

impl ContentStorage {
    pub async fn new(base_path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let root = base_path.into().join("contents");
        tokio::fs::create_dir_all(&root).await?;
        // Staging files whose writer died before the rename have no other reaper.
        //
        // Off the critical path deliberately: this walks every shard directory,
        // and a full content store is 65,536 of them -- 54s of the 89s this
        // process used to take to reach its listener, with the port unbound and
        // the front answering 502 for the whole window. Nothing here is a
        // precondition for serving: it deletes only files older than
        // STAGING_ORPHAN_AGE (1h), which is the same threshold that already made
        // it safe to run against a live store, so running it beside startup
        // rather than before it is safe for exactly the same reason.
        let sweep_root = root.clone();
        tokio::spawn(async move {
            crate::sweep_stale_staging(&sweep_root, "content").await;
        });
        debug!(root = %root.display(), "content storage initialized");
        Ok(Self {
            known_shards: KnownShards::new(root.clone()),
            root,
        })
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub async fn store(&self, hash: &str, data: Bytes) -> Result<(), StorageError> {
        use tokio::io::AsyncWriteExt;

        // The one path that creates the shard directory.
        let path = ensure_file_path(&self.root, hash, &self.known_shards).await?;

        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        // The guard arrives with the file, so from the instant the staging file exists every exit
        // -- an error, a `?`, or this future being dropped mid-write -- removes it.
        let (mut file, mut staging) =
            create_staging_file(staging_path(&path, "content", seq)).await?;
        file.write_all(&data).await?;
        file.sync_all().await?;
        drop(file);

        tokio::fs::rename(staging.path(), &path).await?;
        staging.disarm();

        if let Some(parent) = path.parent() {
            if let Ok(dir) = tokio::fs::File::open(parent).await {
                let _ = dir.sync_all().await;
            }
        }

        debug!(hash, bytes = data.len(), "content stored");
        Ok(())
    }

    pub async fn retrieve(&self, hash: &str) -> Result<Option<Bytes>, StorageError> {
        let path = resolve_file_path(&self.root, hash)?;

        if stat_for_read(&self.known_shards, &path).await?.is_some() {
            let data = tokio::fs::read(&path).await?;
            return Ok(Some(Bytes::from(data)));
        }

        Ok(None)
    }

    pub async fn retrieve_uncompressed(&self, hash: &str) -> Result<Option<Bytes>, StorageError> {
        // Content is stored decompressed, so the "uncompressed" read is the same lookup as
        // `retrieve` -- kept as its own method because callers name their intent explicitly.
        self.retrieve(hash).await
    }

    pub async fn exist(&self, hash: &str) -> Result<bool, StorageError> {
        let path = resolve_file_path(&self.root, hash)?;
        Ok(stat_for_read(&self.known_shards, &path).await?.is_some())
    }

    /// Batch existence probe: a provably-invalid id is a per-id miss that never poisons the batch; only real storage faults abort it.
    pub async fn exist_multiple(
        &self,
        hashes: &[&str],
    ) -> Result<Vec<(String, bool)>, StorageError> {
        use futures::future::BoxFuture;
        use futures::stream::{self, StreamExt};

        // `buffered` (NOT `buffer_unordered`) runs up to 32 stats concurrently while yielding in
        // input order, so the result Vec matches a serial walk with no re-association bookkeeping.
        // The futures are boxed and collected EAGERLY: keeping a lazy `Iterator::map` closure over
        // the borrowed `&&str` items alive inside this async fn's state makes the async-trait caller
        // require it to be higher-ranked-lifetime general ("FnOnce is not general enough"). Collecting
        // consumes the closure here, so only the owned Vec of boxed futures crosses the await points.
        let futs: Vec<BoxFuture<'_, (&str, Result<bool, StorageError>)>> = hashes
            .iter()
            .map(|&hash| Box::pin(async move { (hash, self.exist(hash).await) }) as _)
            .collect();
        let mut st = stream::iter(futs).buffered(32);

        let mut results = Vec::with_capacity(hashes.len());
        while let Some((hash, res)) = st.next().await {
            let exists = match res {
                Ok(exists) => exists,
                Err(StorageError::InvalidId(_)) | Err(StorageError::PathTraversal(_)) => false,
                Err(e) => return Err(e),
            };
            results.push((hash.to_owned(), exists));
        }
        Ok(results)
    }

    /// Best-effort delete: unlink failures other than already-gone are only logged; use [`delete_strict`](Self::delete_strict) to observe the outcome.
    pub async fn delete(&self, hash: &str) -> Result<(), StorageError> {
        let path = resolve_file_path(&self.root, hash)?;

        if let Err(e) = tokio::fs::remove_file(&path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!(hash, error = %e, "failed to delete content file");
            }
        }

        debug!(hash, "content deleted");
        Ok(())
    }

    /// `Ok(())` proves the path no longer holds the content; already-gone counts as success.
    pub async fn delete_strict(&self, hash: &str) -> Result<(), StorageError> {
        let path = resolve_file_path(&self.root, hash)?;

        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        debug!(hash, "content deleted");
        Ok(())
    }

    pub async fn file_path(&self, hash: &str) -> Result<Option<(PathBuf, bool)>, StorageError> {
        let path = resolve_file_path(&self.root, hash)?;

        if stat_for_read(&self.known_shards, &path).await?.is_some() {
            return Ok(Some((path, false)));
        }

        Ok(None)
    }

    /// Opens the content for streaming, returning the file and its size.
    ///
    /// Prefer this over `file_path()` + your own `File::open`: the two-step version has to invent an
    /// answer for an `ENOENT` the stat said was impossible, and answering `absent` there reports a
    /// shard destroyed between the two syscalls as a legitimate 404. Here the stat is an `fstat` of
    /// the descriptor being handed back, so there is no window between the check and the read.
    pub async fn open_for_read(
        &self,
        hash: &str,
    ) -> Result<Option<(tokio::fs::File, u64)>, StorageError> {
        let path = resolve_file_path(&self.root, hash)?;

        Ok(open_for_read(&self.known_shards, &path)
            .await?
            .map(|(file, meta)| (file, meta.len())))
    }

    /// The bytes in `start..=end`, or `None` when the id is absent or the window starts past its end.
    ///
    /// Reads the WINDOW, not the file. Slicing a range out of a whole-file read means a 30 MB model
    /// resident to answer a 32-byte sniff, and one such read per concurrent request; the descriptor
    /// this seeks on is the same one [`open_for_read`](Self::open_for_read) decides absence from, so
    /// the size the window is clamped against is the file being read rather than an earlier stat's.
    pub async fn read_range(
        &self,
        hash: &str,
        start: u64,
        end: u64,
    ) -> Result<Option<Bytes>, StorageError> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let path = resolve_file_path(&self.root, hash)?;
        let Some((mut file, meta)) = open_for_read(&self.known_shards, &path).await? else {
            return Ok(None);
        };

        let size = meta.len();
        let end = end.min(size.saturating_sub(1));
        if start > end || start >= size {
            return Ok(None);
        }

        file.seek(std::io::SeekFrom::Start(start)).await?;
        let mut buf = vec![0u8; (end - start + 1) as usize];
        file.read_exact(&mut buf).await?;

        Ok(Some(Bytes::from(buf)))
    }

    /// The CIDv1 the stored bytes actually hash to, or `None` when the id is absent.
    ///
    /// A key in this store is a claim about its own content, and nothing enforces the claim at
    /// write time for content that arrived from anywhere but [`store`](Self::store). This is how a
    /// caller checks the claim rather than assuming it: `exist()` answers "is there a file here",
    /// which is a different and much weaker question. Streams the file, so a snapshot of hundreds
    /// of megabytes costs a buffer, not its length.
    pub async fn stored_content_hash(&self, hash: &str) -> Result<Option<String>, StorageError> {
        use tokio::io::AsyncReadExt;

        let path = resolve_file_path(&self.root, hash)?;
        let Some((mut file, _)) = open_for_read(&self.known_shards, &path).await? else {
            return Ok(None);
        };

        let mut writer = catalyrst_hashing::HashV1Writer::new();
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            writer.update(&buf[..n]);
        }

        Ok(Some(writer.finish()))
    }

    /// Moves stored content from one id to another, reporting whether there was anything to move.
    ///
    /// For content whose key turned out to misdescribe it: the bytes are worth keeping and the key
    /// is not. A rename settles that without re-reading a file that can run to hundreds of
    /// megabytes, and it retires the wrong key in the same step -- leaving it in place would keep
    /// serving bytes under a CID they do not hash to, which is the whole defect being repaired.
    pub async fn rekey(&self, from: &str, to: &str) -> Result<bool, StorageError> {
        let src = resolve_file_path(&self.root, from)?;
        let dst = ensure_file_path(&self.root, to, &self.known_shards).await?;

        match tokio::fs::rename(&src, &dst).await {
            Ok(()) => {
                if let Some(parent) = dst.parent() {
                    if let Ok(dir) = tokio::fs::File::open(parent).await {
                        let _ = dir.sync_all().await;
                    }
                }
                debug!(from, to, "content re-keyed");
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn file_info(&self, hash: &str) -> Result<Option<FileInfo>, StorageError> {
        let path = resolve_file_path(&self.root, hash)?;

        if let Some(meta) = stat_for_read(&self.known_shards, &path).await? {
            return Ok(Some(FileInfo {
                size: meta.len(),
                encoding: None,
                content_size: Some(meta.len()),
            }));
        }

        Ok(None)
    }

    /// Every id this store actually holds, pulled one at a time -- see [`FileIds`] for the contract.
    pub fn all_file_ids<'a>(&'a self, prefix: Option<&'a str>) -> FileIds<'a> {
        FileIds::new(&self.root, &self.known_shards, "content", prefix)
    }
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub size: u64,
    pub encoding: Option<String>,
    pub content_size: Option<u64>,
}

#[cfg(test)]
mod tests;
