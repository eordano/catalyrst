use std::path::Path;
use tracing::warn;

use crate::{hex_prefix, is_canonical_content_id, KnownShards, StorageError};

#[cfg(test)]
mod tests;

/// A cursor over every id a store actually holds, pulled one at a time.
///
/// NOT a `Vec`: where the ids pile up is the caller's decision, not this crate's. A list built here
/// is one string per id of the WHOLE corpus resident before the consumer sees the first one -- linear
/// in the node's content, on a walk whose only in-flight state need be the two directory handles it
/// is reading. A GC or sync consumer that wants a set can still build one; one that wants to act on
/// each id as it arrives, or to stop early, no longer pays for the rest.
///
/// AT LEAST ONCE, and only ids the point lookups accept. Every id present for the whole enumeration
/// is yielded, and nothing is yielded that `exist()` would then deny -- a consumer that syncs or GCs
/// from this list acting on a phantom deletes real content elsewhere. It is NOT guaranteed to be a
/// set: a store committing during the walk renames its staging file onto the id's name inside a
/// directory this walk is reading, and `readdir(2)` may report an entry renamed under it twice, so a
/// consumer acting on the output must be idempotent. The direction is deliberate -- enumerating an id
/// twice costs an idempotent repeat, while missing one under-reports what the node holds.
///
/// A FAULT IS NEVER AN ENDING. A directory this walk cannot read is reported as an error, never as
/// the `None` that means "there is nothing more here" and never as a quietly shorter list: a
/// consumer diffing a list short by one shard against a peer reads live content as absent from this
/// node. Once a fault is reported the cursor stays failed, so a caller that keeps pulling is told
/// again rather than handed an ending it would take for a complete answer.
pub struct FileIds<'a> {
    root: &'a Path,
    known: &'a KnownShards,
    kind: &'static str,
    prefix: Option<&'a str>,
    shards: Option<tokio::fs::ReadDir>,
    shard_name: String,
    entries: Option<tokio::fs::ReadDir>,
    misplaced: usize,
    failed: bool,
    done: bool,
}

impl<'a> FileIds<'a> {
    pub(crate) fn new(
        root: &'a Path,
        known: &'a KnownShards,
        kind: &'static str,
        prefix: Option<&'a str>,
    ) -> Self {
        Self {
            root,
            known,
            kind,
            prefix,
            shards: None,
            shard_name: String::new(),
            entries: None,
            misplaced: 0,
            failed: false,
            done: false,
        }
    }

    /// The next id, or `None` once the walk is exhausted.
    ///
    /// The root listing is opened on the first call, so an unreadable root is reported here rather
    /// than at construction -- and a caller that never pulls opens nothing at all.
    pub async fn next(&mut self) -> Result<Option<String>, StorageError> {
        if self.failed {
            return Err(StorageError::Io(std::io::Error::other(
                "enumeration reported a storage fault: the list it produced stops short of the tree",
            )));
        }
        if self.done {
            return Ok(None);
        }
        if self.shards.is_none() {
            match tokio::fs::read_dir(self.root).await {
                Ok(shards) => self.shards = Some(shards),
                Err(e) => return Err(self.fail(e)),
            }
        }

        loop {
            let step = match self.entries.as_mut() {
                Some(entries) => Some(entries.next_entry().await),
                None => None,
            };
            match step {
                Some(Ok(Some(entry))) => {
                    if let Some(id) = self.id_of(entry).await {
                        return Ok(Some(id));
                    }
                    continue;
                }
                Some(Ok(None)) => {
                    self.entries = None;
                    continue;
                }
                Some(Err(e)) => return Err(self.fail(e)),
                None => {}
            }

            let shards = match self.shards.as_mut() {
                Some(shards) => shards,
                None => return Ok(None),
            };
            let stepped = shards.next_entry().await;
            let shard = match stepped {
                Ok(Some(shard)) => shard,
                Ok(None) => {
                    self.finish();
                    return Ok(None);
                }
                Err(e) => return Err(self.fail(e)),
            };

            let listed = shard.file_type().await.ok();
            let proven_directory = listed.is_some_and(|ft| ft.is_dir());
            let shard_path = shard.path();
            // Only a name a hashed id resolves into can cost the listing an id, so damage to
            // anything else at the root is not this walk's to report -- and reporting it would let
            // one operator scratch file cost the whole corpus its listing.
            let shardable = KnownShards::names_a_shard(&shard_path);
            // Only a directory can hold ids, and the listing is asked to PROVE the entry is not one
            // rather than to prove it is: a symlinked shard reads back as the link and an entry
            // whose type the listing does not report reads back as nothing at all, and dropping
            // either loses every id underneath it while all their point lookups keep working.
            if listed.is_some_and(|ft| !ft.is_dir() && !ft.is_symlink()) {
                if shardable {
                    return Err(self.fail(occupied_shard(&shard_path)));
                }
                continue;
            }
            match tokio::fs::read_dir(&shard_path).await {
                Ok(entries) => {
                    // Opening it proves it is a directory, and enumeration is part of the same read
                    // contract as a point lookup: a shard this walk listed must not read as
                    // never-having-existed if it disappears afterwards.
                    self.known.remember(&shard_path);
                    self.shard_name = shard.file_name().to_string_lossy().to_string();
                    self.entries = Some(entries);
                }
                Err(_) if !shardable => continue,
                Err(e) if !proven_directory && vanished_unobserved(self.known, &shard_path, &e) => {
                    continue
                }
                Err(e) => return Err(self.fail(e)),
            }
        }
    }

    /// The id an entry stands for, or `None` when it is not one.
    ///
    /// Three conditions, and the third is the round trip: a canonical id, an occupant a read can
    /// serve, and sitting in the shard its own hash selects. A file moved (or restored) into the
    /// wrong shard is unreachable by id -- every read hashes the id to the OTHER shard -- so yielding
    /// it hands a sweep a name whose lookup resolves somewhere else entirely.
    ///
    /// The occupant is judged POSITIVELY, by what it is rather than by what it is not: a directory,
    /// fifo, socket or device node is skipped because every read of it faults, while a regular file,
    /// a symlink to one, and an entry whose type the listing does not report all stay enumerable,
    /// because that is exactly what the point lookups serve.
    async fn id_of(&mut self, entry: tokio::fs::DirEntry) -> Option<String> {
        let name = entry.file_name().to_string_lossy().to_string();

        if !is_canonical_content_id(&name) {
            return None;
        }
        if entry.file_type().await.is_ok_and(is_foreign_node) {
            return None;
        }
        if hex_prefix(&name) != self.shard_name {
            self.misplaced += 1;
            return None;
        }
        if self.prefix.is_some_and(|pfx| !name.starts_with(pfx)) {
            return None;
        }

        Some(name)
    }

    fn finish(&mut self) {
        self.done = true;
        // The walk is over; a caller holding the cursor should not also be holding the root's handle.
        self.shards = None;
        if self.misplaced > 0 {
            warn!(
                root = %self.root.display(),
                kind = self.kind,
                misplaced = self.misplaced,
                "enumeration skipped entries whose id hashes to another shard: they are unreachable by id"
            );
        }
    }

    /// Ends the walk for good. Everything already yielded stands; what follows is unknown, and the
    /// cursor says so on every later pull rather than offering the ending that reads as completeness.
    fn fail(&mut self, err: std::io::Error) -> StorageError {
        self.failed = true;
        self.shards = None;
        self.entries = None;
        warn!(
            root = %self.root.display(),
            kind = self.kind,
            error = %err,
            "refusing to under-report: enumeration could not read the whole tree"
        );
        err.into()
    }
}

/// Occupants of a content path that no id can have stored and no read can serve.
fn is_foreign_node(ft: std::fs::FileType) -> bool {
    if ft.is_dir() {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        ft.is_fifo() || ft.is_socket() || ft.is_char_device() || ft.is_block_device()
    }
    #[cfg(not(unix))]
    false
}

/// The fault a point read of any id in this shard already reports: something occupies the shard path
/// and it is not a directory, so every id that hashes there is unreachable.
fn occupied_shard(path: &Path) -> std::io::Error {
    std::io::Error::other(format!("shard path is not a directory: {}", path.display()))
}

/// Whether a shard the root listing did not prove to be a directory is simply not there, which is
/// the one thing that makes passing it over a complete answer rather than a hidden gap.
///
/// "Nothing is there" is the whole truth for a name this instance never saw be a shard -- a dangling
/// symlink, or an entry removed since the listing -- but for one it did observe it is the same
/// destruction a point read reports, so that is raised rather than passed over.
fn vanished_unobserved(known: &KnownShards, path: &Path, err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::NotFound && !known.contains(path)
}
