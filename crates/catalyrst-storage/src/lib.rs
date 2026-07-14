mod content_storage;
pub mod content_store;
mod file_ids;
mod snapshot_storage;

pub use content_storage::ContentStorage;
pub use file_ids::FileIds;
pub use snapshot_storage::SnapshotStorage;

use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::warn;

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

/// Whether `id` is a content hash this storage will accept as a key (CIDv0 `Qm...` base58 or
/// CIDv1 `ba...` base32). Public so ingestion layers can discard remote metadata whose hashes
/// storage would reject anyway, instead of failing later at download time.
pub fn is_canonical_content_id(id: &str) -> bool {
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

/// Resolves an id to its canonical path, validating it, WITHOUT touching the filesystem.
///
/// Every read path uses this, as do `delete`/`delete_strict`. Reads leave the tree untouched: a
/// resolve that created the shard would HEAL a destroyed one before every probe, so the damage would
/// answer as an ordinary miss for every id in that shard. [`ensure_file_path`] is the write-only
/// variant that also creates the parent.
pub(crate) fn resolve_file_path(root: &Path, id: &str) -> Result<PathBuf, StorageError> {
    if !is_canonical_content_id(id) {
        return Err(StorageError::InvalidId(id.to_string()));
    }

    let prefix = hex_prefix(id);
    let dir = root.join(&prefix);
    let file_path = dir.join(id);

    let normalized = file_path.components().fold(PathBuf::new(), |mut acc, c| {
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

    Ok(file_path)
}

/// Resolves an id AND ensures its shard directory exists. For WRITE paths only -- see
/// [`resolve_file_path`] for why reads must not create anything.
///
/// The shard is recorded in `known` at the moment its existence is proven, which is what lets a
/// later disappearance be classified as damage rather than as "nothing was ever stored here".
pub(crate) async fn ensure_file_path(
    root: &Path,
    id: &str,
    known: &KnownShards,
) -> Result<PathBuf, StorageError> {
    let file_path = resolve_file_path(root, id)?;

    if let Some(dir) = file_path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    known.remember_parent(&file_path);

    Ok(file_path)
}

/// The shard directories this instance has created or observed intact.
///
/// Files live under a directory named by the first four hex digits of the id's SHA-1 (see
/// [`hex_prefix`]), so the whole key space is the 65,536 values of a `u16` and the set is a fixed
/// 8 KiB bitmap: no allocation and no cap. Upstream needs `MAX_TRACKED_DIRECTORIES` plus a
/// FIFO eviction fallback because its flat mode lets ids nest arbitrarily; sharding is the only
/// layout we have, and 65,536 entries never reach a budget that would need one.
///
/// APPEND-ONLY. An observation is evidence about the past -- this instance saw that directory be a
/// directory -- and nothing that happens later makes it untrue, so nothing clears it. Consuming the
/// evidence to report the damage derived from it makes the FIRST read of a destroyed shard fault and
/// every read after it answer "absent" over an unchanged disk, which is the silent data-loss report
/// this whole unit exists to refuse; with concurrent readers it is worse, since one of them takes the
/// fault and the rest are told the content is simply gone. The report is stable instead: it lasts
/// exactly as long as the damage, and the store that recreates the shard ends it. Nothing here is a
/// mkdir-skip cache -- [`ensure_file_path`] creates the tree unconditionally -- so no write path
/// depends on an entry being droppable.
///
/// Concurrency: a plain array of `AtomicU64`, deliberately not a `Mutex<HashSet<_>>`. The storage
/// structs are shared across tasks behind an `Arc` and every read consults this set, so a lock would
/// put a contended critical section on the hot path and would be one more thing that must never be
/// held across an `.await`; a fixed bitmap needs neither -- each membership test is one relaxed load
/// and each mutation one relaxed `fetch_or`. `Relaxed` is sufficient because the bits are
/// independent and publish no other memory: a stale load can only produce a MISS where a fault was
/// possible, never a fault where the truth is a miss.
#[derive(Debug)]
pub(crate) struct KnownShards {
    /// The storage root, which is observed BY CONSTRUCTION: `new()` is called on a root this process
    /// has just created, so a read that finds it gone is looking at a destroyed tree rather than at a
    /// node that never held anything. Kept out of the bitmap on purpose -- a 4-hex slot is the only
    /// thing that fits there, and evidence that can be lost is evidence that decides the answer wrong.
    root: PathBuf,
    /// 65,536 bits, one per possible 4-hex shard name.
    words: [AtomicU64; 1024],
}

impl KnownShards {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            words: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// `(word index, bit mask)` for a shard directory, or `None` if the final component is not a
    /// name [`hex_prefix`] can produce -- an unshardable path is simply never remembered, so it
    /// degrades to the conservative answer (a miss) rather than to a bogus fault.
    ///
    /// LOWERCASE ONLY, because that is the whole alphabet `hex_prefix` emits and enumeration feeds
    /// this log: a foreign `F049` restored from a case-preserving volume is a directory no id
    /// resolves into, and folding it onto `f049`'s slot would have it vouch for a shard nothing was
    /// ever stored in -- manufacturing damage for a read of an id that is merely absent.
    fn slot(dir: &Path) -> Option<(usize, u64)> {
        let name = dir.file_name()?.to_str()?;
        let canonical_hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
        if name.len() != 4 || !name.bytes().all(canonical_hex) {
            return None;
        }
        let index = usize::from_str_radix(name, 16).ok()?;
        Some((index / 64, 1u64 << (index % 64)))
    }

    /// Whether this path names a shard at all -- the only names an id can resolve into, and so the
    /// only ones whose damage costs the store a reachable id.
    pub(crate) fn names_a_shard(dir: &Path) -> bool {
        Self::slot(dir).is_some()
    }

    /// Records a shard directory as observed to exist. Opening one, statting through one or listing
    /// one all prove the same thing, so every path that learns it goes through here.
    pub(crate) fn remember(&self, dir: &Path) {
        if let Some((word, bit)) = Self::slot(dir) {
            self.words[word].fetch_or(bit, Ordering::Relaxed);
        }
    }

    /// Records the shard holding `file_path` as observed to exist.
    pub(crate) fn remember_parent(&self, file_path: &Path) {
        if let Some(dir) = file_path.parent() {
            self.remember(dir);
        }
    }

    /// Did this instance ever create or observe this shard directory?
    pub(crate) fn contains(&self, dir: &Path) -> bool {
        Self::slot(dir)
            .is_some_and(|(word, bit)| self.words[word].load(Ordering::Relaxed) & bit != 0)
    }

    /// Did this instance ever create or observe the shard holding `file_path`?
    pub(crate) fn parent_known(&self, file_path: &Path) -> bool {
        file_path.parent().is_some_and(|dir| self.contains(dir))
    }
}

/// Miss only when the file provably cannot exist; every other stat error is a fault.
pub(crate) async fn stat_for_read(
    known: &KnownShards,
    path: &Path,
) -> Result<Option<std::fs::Metadata>, StorageError> {
    match tokio::fs::metadata(path).await {
        Ok(meta) if meta.is_file() => {
            // Statting a file PROVES its parent is an intact directory -- nothing can be statted
            // inside a path that is not one. Recording that is what lets a LATER disappearance of
            // the same shard read as damage. Load-bearing now that reads no longer create
            // directories: an instance that only ever READS would otherwise never learn which
            // shards exist and would answer a destroyed shard with "absent" for every id in it.
            known.remember_parent(path);
            Ok(Some(meta))
        }
        Ok(_) => {
            // The stat SUCCEEDED, so the shard is provably an intact directory -- learn that even
            // though this particular path is unusable: the evidence is about the parent, not about
            // whatever occupies the path inside it.
            known.remember_parent(path);
            warn!(path = %path.display(), "storage path is not a regular file");
            Err(StorageError::Io(std::io::Error::other(
                "storage path is not a regular file",
            )))
        }
        // ENOENT, plus ENOTDIR/ENAMETOOLONG: no file of that name can exist at that path. Which of
        // the two very different meanings of "absent" this is, only the parent can say.
        Err(e) if is_provably_absent(&e) => classify_absence(known, path, e).await,
        Err(e) => Err(e.into()),
    }
}

fn is_provably_absent(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::NotFound {
        return true;
    }
    #[cfg(unix)]
    {
        matches!(
            e.raw_os_error(),
            Some(libc::ENOTDIR) | Some(libc::ENAMETOOLONG)
        )
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Opens a file for reading under the SAME miss-vs-fault decision as [`stat_for_read`].
///
/// One decision, not two: the caller that stats and then opens has to invent an answer for an
/// `ENOENT` from the open that the stat said could not happen, and the available answer is
/// `Ok(None)` -- reporting a file that vanished between the two syscalls, or a shard destroyed
/// between them, as provable absence. That is the contract inversion this unit refuses. Opening
/// first and `fstat`-ing the descriptor also removes the TOCTOU window entirely: the metadata
/// describes the file the caller will actually read.
pub(crate) async fn open_for_read(
    known: &KnownShards,
    path: &Path,
) -> Result<Option<(tokio::fs::File, std::fs::Metadata)>, StorageError> {
    let mut opts = tokio::fs::OpenOptions::new();
    opts.read(true);
    // O_NONBLOCK, or the fstat below is never reached. `open(2)` on a FIFO with no writer BLOCKS
    // until one appears -- forever, in practice -- and tokio runs it on the blocking pool, so a single
    // FIFO left at a content path burned one pool thread per request against that id (default cap
    // 512) and stopped the runtime from shutting down. It is a no-op for regular files, which are
    // the only descriptors this function ever hands back.
    #[cfg(unix)]
    opts.custom_flags(libc::O_NONBLOCK);

    match opts.open(path).await {
        Ok(file) => {
            let meta = file.metadata().await?;
            // A directory (or a FIFO, or a device) opens fine; only the read misbehaves, and by then
            // the response has started. Same fault `stat_for_read` reports, decided before any body
            // is streamed.
            known.remember_parent(path);
            if !meta.is_file() {
                warn!(path = %path.display(), "storage path is not a regular file");
                return Err(StorageError::Io(std::io::Error::other(
                    "storage path is not a regular file",
                )));
            }
            Ok(Some((file, meta)))
        }
        Err(e) if is_provably_absent(&e) => classify_absence(known, path, e).await.map(|_| None),
        Err(e) => Err(e.into()),
    }
}

/// Decides whether a file that is not there is an ordinary miss or a damaged store.
///
/// Costs one syscall, and only after a stat has already failed -- hits, the hot path, are untouched.
/// A second one is spent only when the shard itself is missing, which is the branch that has to tell
/// "nothing was ever stored here" from "the tree was destroyed".
///
/// The answer is DERIVED from the tree on every call, never from a report already made: an
/// observation is not consumed by the fault it justifies, so the same damaged id answers the same
/// way on the first read, the tenth, and to sixteen readers at once. Two surfaces over one root that
/// have seen the same directories cannot disagree about it either.
async fn classify_absence(
    known: &KnownShards,
    path: &Path,
    err: std::io::Error,
) -> Result<Option<std::fs::Metadata>, StorageError> {
    let Some(dir) = path.parent() else {
        return Ok(None);
    };

    match tokio::fs::metadata(dir).await {
        // An intact shard that simply does not hold this file: the ordinary miss. Remembered for the
        // same reason a successful stat is -- it is proof the shard exists right now.
        Ok(meta) if meta.is_dir() => {
            known.remember(dir);
            Ok(None)
        }
        // Something is AT the shard path but is not a directory. Never a legitimate empty state -- no
        // id resolves to a 4-hex name, so nothing this storage writes can put a file, a fifo or a
        // device node there, and whatever is there makes every id in the shard unreadable. Nothing on
        // disk is removed: destroying something this storage cannot prove it owns is exactly what the
        // id-validation rules refuse to do.
        Ok(_) => {
            warn!(path = %path.display(), "refusing to report absence: the shard path is not a directory");
            Err(err.into())
        }
        Err(probe) if probe.kind() == std::io::ErrorKind::NotFound => {
            // The shard is gone. Reads no longer create it, so for a shard nothing was ever stored
            // in this is the normal answer. It is a FAULT when this instance created or observed that
            // directory, which means the tree it owns was destroyed underneath it, taking every id
            // inside with it.
            if known.parent_known(path) {
                warn!(path = %path.display(), "refusing to report absence: the shard directory was removed underneath us");
                return Err(err.into());
            }
            // A shard this instance never saw proves nothing on its own, so the walk continues to the
            // one directory whose existence is not in question: this process created the root, so a
            // root that is gone (or unreadable, or no longer a directory) is a destroyed store
            // answering like an empty one -- for the whole key space, not one shard of it.
            match tokio::fs::metadata(known.root()).await {
                Ok(meta) if meta.is_dir() => Ok(None),
                _ => {
                    warn!(root = %known.root().display(), "refusing to report absence: the storage root is gone");
                    Err(err.into())
                }
            }
        }
        // The shard could not be read at all (EACCES, EIO, or ENOTDIR meaning an ANCESTOR is not a
        // directory). This storage cannot answer the question and must not pretend the id is absent.
        Err(_) => {
            warn!(path = %path.display(), "refusing to report absence: the shard directory could not be read");
            Err(err.into())
        }
    }
}

/// Suffix of a staging file: a write in progress, never addressable content (no canonical id
/// contains a `.`, so enumeration filters these out by the same rule that rejects any other junk).
pub(crate) const STAGING_SUFFIX: &str = ".tmp";

/// How old a staging file must be before the startup sweep treats it as an orphan.
///
/// Staging names carry the writer's pid, but pid reuse makes "is that process alive" unreliable, so
/// age is the discriminator. Unlinking a live writer's staging file does not corrupt anything -- on
/// POSIX its descriptor stays valid against the now-unnamed inode and the write completes into it --
/// but the store then FAILS at the rename with `ENOENT`, so a careless sweep turns another process's
/// healthy in-flight write into a spurious error. The threshold buys a margin far beyond any
/// plausible single-file write (a multi-GB asset onto a slow disk) and matches upstream's
/// `ONE_HOUR_IN_MS`, which governs the same class of reclamation.
///
/// The sweep is also the backstop for what [`create_staging_file`]'s hand-off cannot cover: a
/// cancellation during runtime shutdown, where the task that owns the guard may never be polled
/// again, leaves residue that only the next start reclaims.
pub(crate) const STAGING_ORPHAN_AGE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Builds the staging path a store writes before committing with a rename.
pub(crate) fn staging_path(final_path: &Path, fallback_stem: &str, seq: u64) -> PathBuf {
    let base = final_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(fallback_stem);
    final_path.with_file_name(format!(
        "{}.{}.{}{}",
        base,
        std::process::id(),
        seq,
        STAGING_SUFFIX
    ))
}

/// Unlinks a staging file unless the write committed.
///
/// Every escape from `store()` has to remove it, and only the `write_all` error path used to: a
/// `sync_all` or `rename` failure leaked it, and so did the whole future being DROPPED -- axum drops
/// a handler's future the moment the client disconnects, and `sync_all` on a large asset is a wide
/// window. Nothing else in the workspace reaps them, so a leaked staging file lived forever and
/// enumeration then offered it as an id.
pub(crate) struct StagingGuard {
    path: PathBuf,
    armed: bool,
}

impl StagingGuard {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The rename committed the bytes, so there is no staging file left to remove.
    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // Blocking unlink on purpose. `Drop` cannot await, and handing the work to the runtime would
        // lose it in exactly the case that matters most -- a cancelled future during shutdown. One
        // unlink in a directory that is already hot costs microseconds.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Creates the staging file, handing back the open file together with the guard that owns it.
///
/// The create runs in a DETACHED task, and the guard travels THROUGH the channel, because a guard
/// held by the caller cannot cover this step: tokio's fs calls run on the blocking pool and keep
/// running after the awaiting future is dropped, so a store cancelled here went on to create the
/// file milliseconds after the only thing that could remove it was gone. Reproduced -- a 1 ms timeout
/// around `store()` leaked `<id>.<pid>.<seq>.tmp` every time, guard or no guard.
///
/// Ownership is what makes it airtight: the guard never exists apart from the file it names, and the
/// oneshot either delivers both to a caller that is still there or drops both -- running the guard --
/// when the receiver is gone. There is no instant at which the file exists and its guard does not.
pub(crate) async fn create_staging_file(
    tmp_path: PathBuf,
) -> Result<(tokio::fs::File, StagingGuard), StorageError> {
    let (tx, rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        let mut opts = tokio::fs::OpenOptions::new();
        // O_CREAT|O_EXCL: the staging name is unique to this call (pid plus a per-process counter),
        // so an existing one is never ours to take over. O_NOFOLLOW refuses a planted symlink.
        opts.write(true).create_new(true);
        #[cfg(unix)]
        opts.custom_flags(libc::O_NOFOLLOW);

        match opts.open(&tmp_path).await {
            Ok(file) => {
                let _ = tx.send(Ok((file, StagingGuard::new(tmp_path))));
            }
            // Nothing was created, so there is nothing to guard.
            Err(e) => {
                let _ = tx.send(Err(e));
            }
        }
    });

    match rx.await {
        Ok(Ok(opened)) => Ok(opened),
        Ok(Err(e)) => Err(e.into()),
        // The task always sends; the sender only vanishes if it panicked or the runtime is shutting
        // down. Either way this store did not happen, and any file it made is guarded.
        Err(_) => Err(StorageError::Io(std::io::Error::other(
            "staging file creation did not complete",
        ))),
    }
}

/// Removes staging files left behind by writers that died before their rename.
///
/// Best effort by construction: every error is skipped, because a store that cannot be swept is
/// still a store that must open. Costs one `readdir` per EXISTING shard, once per process -- nothing
/// on a fresh root, and bounded by 65,536 on a full one.
pub(crate) async fn sweep_stale_staging(root: &Path, kind: &str) -> usize {
    let started = std::time::Instant::now();
    let mut swept = 0usize;

    let Ok(mut shards) = tokio::fs::read_dir(root).await else {
        return 0;
    };

    while let Ok(Some(shard)) = shards.next_entry().await {
        if !matches!(shard.file_type().await, Ok(ft) if ft.is_dir()) {
            continue;
        }
        let Ok(mut entries) = tokio::fs::read_dir(shard.path()).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if !entry
                .file_name()
                .to_string_lossy()
                .ends_with(STAGING_SUFFIX)
            {
                continue;
            }
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let old_enough = meta
                .modified()
                .ok()
                .and_then(|m| std::time::SystemTime::now().duration_since(m).ok())
                .is_some_and(|age| age >= STAGING_ORPHAN_AGE);
            if !old_enough {
                continue;
            }
            if tokio::fs::remove_file(entry.path()).await.is_ok() {
                swept += 1;
            }
        }
    }

    if swept > 0 {
        warn!(
            kind,
            root = %root.display(),
            swept,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "removed orphaned staging files from a previous run"
        );
    }
    swept
}

/// Staging files still present in `shard_dir` after cleanup has had a chance to run.
///
/// Cleanup is eventually-consistent BY CONSTRUCTION: the guard rides back through a oneshot owned by
/// a detached task, so the unlink lands when that task is next polled, not at the instant the caller
/// is cancelled. Sampling immediately therefore reads a race -- measured to still see the file in
/// well under 1% of observations -- and would be flaky in both directions.
#[cfg(test)]
pub(crate) async fn wait_for_staging_cleanup(shard_dir: &Path) -> Vec<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    loop {
        let mut leaked = Vec::new();
        if let Ok(mut entries) = tokio::fs::read_dir(shard_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(STAGING_SUFFIX) {
                    leaked.push(name);
                }
            }
        }
        if leaked.is_empty() || std::time::Instant::now() >= deadline {
            return leaked;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
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
        let normalized = file_path.components().fold(PathBuf::new(), |mut acc, c| {
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

    /// The observation log records one shard without touching its neighbours, and never unlearns.
    #[test]
    fn known_shards_record_one_shard_and_keep_it() {
        let known = KnownShards::new(PathBuf::from("/root"));
        let a = PathBuf::from("/root/f049/id-a");
        let b = PathBuf::from("/root/0ac7/id-b");

        known.remember_parent(&a);
        assert!(known.parent_known(&a));
        assert!(
            !known.parent_known(&b),
            "a neighbouring shard is not implied by this one"
        );

        known.remember_parent(&b);
        assert!(
            known.parent_known(&a) && known.parent_known(&b),
            "nothing an observation records is ever taken back"
        );
    }

    #[test]
    fn known_shards_ignores_unshardable_paths() {
        let known = KnownShards::new(PathBuf::from("/root"));
        let odd = PathBuf::from("/root/not-a-shard/id");
        known.remember_parent(&odd);
        assert!(
            !known.parent_known(&odd),
            "a non-4-hex parent is never remembered, so it can never manufacture a fault"
        );

        // A foreign directory spelled in the case `hex_prefix` never emits. Crediting it would let
        // an operator's copy answer for the shard the ids actually live in.
        let shouted = PathBuf::from("/root/F049/id");
        known.remember_parent(&shouted);
        assert!(!known.parent_known(&shouted));
        assert!(
            !known.parent_known(&PathBuf::from("/root/f049/id")),
            "and it must not vouch for the shard whose name it folds onto"
        );
        assert!(!KnownShards::names_a_shard(Path::new("/root/F049")));
        assert!(KnownShards::names_a_shard(Path::new("/root/f049")));
    }

    #[tokio::test]
    async fn staging_guard_removes_the_file_unless_disarmed() {
        let dir = std::env::temp_dir().join(format!("catalyrst-guard-{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let dropped = dir.join("dropped.tmp");
        tokio::fs::write(&dropped, b"x").await.unwrap();
        drop(StagingGuard::new(dropped.clone()));
        assert!(!dropped.exists(), "an armed guard unlinks on drop");

        let committed = dir.join("committed.tmp");
        tokio::fs::write(&committed, b"x").await.unwrap();
        let mut guard = StagingGuard::new(committed.clone());
        guard.disarm();
        drop(guard);
        assert!(
            committed.exists(),
            "a disarmed guard leaves the committed file alone"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn sweep_removes_old_staging_files_but_not_fresh_ones() {
        let root = std::env::temp_dir().join(format!("catalyrst-sweep-{}", std::process::id()));
        let shard = root.join("f049");
        tokio::fs::create_dir_all(&shard).await.unwrap();

        let stale = shard.join("some-id.999.0.tmp");
        let fresh = shard.join("some-id.999.1.tmp");
        let content = shard.join("bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa");
        for p in [&stale, &fresh, &content] {
            tokio::fs::write(p, b"x").await.unwrap();
        }

        let old =
            std::time::SystemTime::now() - STAGING_ORPHAN_AGE - std::time::Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();

        assert_eq!(sweep_stale_staging(&root, "test").await, 1);
        assert!(!stale.exists(), "an orphan past the threshold is removed");
        assert!(
            fresh.exists(),
            "a staging file young enough to be a live write from another process is kept"
        );
        assert!(content.exists(), "real content is never touched");

        let _ = tokio::fs::remove_dir_all(&root).await;
    }
}
