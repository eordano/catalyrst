use super::*;
use crate::ContentStorage;
use bytes::Bytes;
use std::path::PathBuf;

/// Drains the walk into a `Vec`, which is what a test wants and what the walk itself refuses to
/// decide for its callers.
async fn collect_ids(storage: &ContentStorage) -> Vec<String> {
    let mut walk = storage.all_file_ids(None);
    let mut ids = Vec::new();
    while let Some(id) = walk.next().await.unwrap() {
        ids.push(id);
    }
    ids
}

fn shard_dir_of(storage: &ContentStorage, hash: &str) -> PathBuf {
    crate::resolve_file_path(storage.root(), hash)
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[cfg(unix)]
fn running_as_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// Every point lookup follows a symlink at a content path and serves what it finds, so the listing
/// has to offer the id too: a dirent reports the LINK rather than its target, and judging the entry
/// by "is it a regular file" drops content this node holds and answers for.
#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_content_file_is_enumerated_like_the_file_it_points_at() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-linkid-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let target = tmp.join("outside-target");
    tokio::fs::write(&target, b"real bytes").await.unwrap();

    let path = crate::resolve_file_path(storage.root(), hash).unwrap();
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();

    assert!(
        storage.exist(hash).await.unwrap(),
        "the point lookup follows the link"
    );
    assert_eq!(
        storage.retrieve(hash).await.unwrap().unwrap(),
        Bytes::from_static(b"real bytes")
    );
    assert_eq!(
        collect_ids(&storage).await,
        vec![hash.to_string()],
        "so a listing that omits it under-reports what this node serves"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// The occupants no read can serve are the ones enumeration drops. A fifo, a socket and a directory
/// at a content path all fault on every point lookup, so yielding one hands a GC sweep an id whose
/// `exist()` throws -- a batch that fails again on every retry.
#[cfg(unix)]
#[tokio::test]
async fn foreign_occupants_of_a_content_path_are_never_enumerated() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-foreign-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let stored = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let fifo_id = "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa";
    let dir_id = "bafkreifzjut3te2nhyekklss27nh3k72ysco7y32koao5eei66wof36n5e";
    storage
        .store(stored, Bytes::from_static(b"x"))
        .await
        .unwrap();

    let fifo_path = crate::resolve_file_path(storage.root(), fifo_id).unwrap();
    tokio::fs::create_dir_all(fifo_path.parent().unwrap())
        .await
        .unwrap();
    let c_path = std::ffi::CString::new(fifo_path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(
        unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) },
        0,
        "failed to create the test FIFO"
    );
    tokio::fs::create_dir_all(crate::resolve_file_path(storage.root(), dir_id).unwrap())
        .await
        .unwrap();

    let ids = collect_ids(&storage).await;
    assert_eq!(
        ids,
        vec![stored.to_string()],
        "only the id a read can serve is yielded"
    );
    assert!(matches!(
        storage.exist(fifo_id).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.exist(dir_id).await,
        Err(StorageError::Io(_))
    ));

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A shard reached through a symlink serves every id inside it, so the walk descends there too --
/// otherwise one entry the listing reports as a link costs the whole shard, silently.
#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_shard_is_walked_like_the_directory_it_points_at() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-linkshard-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let elsewhere = tmp.join("shard-elsewhere");
    tokio::fs::create_dir_all(&elsewhere).await.unwrap();
    std::os::unix::fs::symlink(&elsewhere, shard_dir_of(&storage, hash)).unwrap();

    storage.store(hash, Bytes::from_static(b"x")).await.unwrap();
    assert!(
        elsewhere.join(hash).exists(),
        "the store landed through the link"
    );
    assert!(storage.exist(hash).await.unwrap());
    assert_eq!(
        collect_ids(&storage).await,
        vec![hash.to_string()],
        "the ids under a symlinked shard are held by this node like any others"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A shard this node cannot read is not a shard holding nothing. The walk reports the fault instead
/// of returning a shorter list, and keeps reporting it: a consumer that diffs a silently short list
/// against a peer reads live content as absent from here and deletes it.
#[cfg(unix)]
#[tokio::test]
async fn an_unreadable_shard_fails_the_walk_instead_of_shortening_it() {
    if running_as_root() {
        eprintln!("skipping: permissions do not bind when running as root");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-walkfault-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    storage.store(hash, Bytes::from_static(b"x")).await.unwrap();
    let shard = shard_dir_of(&storage, hash);
    set_mode(&shard, 0o000);

    let mut walk = storage.all_file_ids(None);
    assert!(
        matches!(walk.next().await, Err(StorageError::Io(_))),
        "an unreadable shard is a fault, not an empty one"
    );
    assert!(
        matches!(walk.next().await, Err(StorageError::Io(_))),
        "and a caller that keeps pulling is told again, never handed the end of the list"
    );
    // The same tree that faults for the walk faults for a point read, which is what makes the two
    // surfaces agree about damage.
    assert!(matches!(
        storage.exist(hash).await,
        Err(StorageError::Io(_))
    ));

    set_mode(&shard, 0o755);
    assert_eq!(collect_ids(&storage).await, vec![hash.to_string()]);

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// What sits at the root and cannot hold ids is passed over, never fatal. Faulting on it would let
/// one stray file -- an operator's copy, a dangling link -- cost the entire corpus its listing, which
/// is a far wider blast radius than the thing it would be reporting.
#[tokio::test]
async fn root_entries_that_cannot_hold_ids_are_passed_over() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-rootjunk-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    storage.store(hash, Bytes::from_static(b"x")).await.unwrap();
    tokio::fs::write(storage.root().join("notes.txt"), b"operator scratch")
        .await
        .unwrap();

    #[cfg(unix)]
    // A 4-hex name, so it is a plausible shard, pointing at nothing: never observed, so its absence
    // is the whole truth about it.
    std::os::unix::fs::symlink(tmp.join("nothing-here"), storage.root().join("0bad")).unwrap();

    assert_eq!(collect_ids(&storage).await, vec![hash.to_string()]);

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A shard path occupied by something that is not a directory is the same damage every point read of
/// an id inside it already reports -- no id resolves to a 4-hex name, so nothing this storage writes
/// can put it there, and it makes the whole shard unreadable. The walk cannot list through it
/// either, so answering "nothing here" would hide the one kind of damage its consumers act on.
#[tokio::test]
async fn a_file_at_a_shard_path_fails_the_walk_like_it_fails_a_read() {
    let tmp =
        std::env::temp_dir().join(format!("catalyrst-test-shardsquat-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let shard = shard_dir_of(&storage, hash);
    tokio::fs::write(&shard, b"not a directory").await.unwrap();

    assert!(
        matches!(storage.exist(hash).await, Err(StorageError::Io(_))),
        "the point read faults without ever having observed the shard"
    );
    let mut walk = storage.all_file_ids(None);
    assert!(
        matches!(walk.next().await, Err(StorageError::Io(_))),
        "so the walk must not report the same shard as holding nothing"
    );
    assert!(
        shard.is_file(),
        "and nothing this storage cannot prove it owns is removed"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// The strictness above is bounded by what an id can reach. A root entry whose name no `hex_prefix`
/// can spell holds nothing this walk could omit, so even damage to it is passed over: one operator
/// scratch directory must not cost the whole corpus its listing.
#[cfg(unix)]
#[tokio::test]
async fn an_unreadable_directory_no_id_hashes_into_is_passed_over() {
    if running_as_root() {
        eprintln!("skipping: permissions do not bind when running as root");
        return;
    }
    let tmp =
        std::env::temp_dir().join(format!("catalyrst-test-foreigndir-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    storage.store(hash, Bytes::from_static(b"x")).await.unwrap();

    let foreign = storage.root().join("operator-backup");
    tokio::fs::create_dir_all(&foreign).await.unwrap();
    set_mode(&foreign, 0o000);

    assert_eq!(collect_ids(&storage).await, vec![hash.to_string()]);

    set_mode(&foreign, 0o755);
    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A shard the walk observed and then lost is damage, so the fault is raised rather than passed over
/// even where the entry type left the walk to find out by opening it.
#[cfg(unix)]
#[tokio::test]
async fn a_vanished_symlinked_shard_is_a_fault_once_it_has_been_observed() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-lostlink-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let elsewhere = tmp.join("shard-elsewhere");
    tokio::fs::create_dir_all(&elsewhere).await.unwrap();
    let shard = shard_dir_of(&storage, hash);
    std::os::unix::fs::symlink(&elsewhere, &shard).unwrap();
    storage.store(hash, Bytes::from_static(b"x")).await.unwrap();
    assert_eq!(collect_ids(&storage).await, vec![hash.to_string()]);

    // The link survives, its target does not: opening the shard now answers ENOENT for a directory
    // this walk has already listed.
    tokio::fs::remove_dir_all(&elsewhere).await.unwrap();
    assert!(
        matches!(
            storage.all_file_ids(None).next().await,
            Err(StorageError::Io(_))
        ),
        "a shard that was there and is gone is destruction, not an empty node"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}
