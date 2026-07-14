use super::*;
use bytes::Bytes;

/// Drains the walk into a `Vec`, which is what a test wants and what the walk itself refuses to
/// decide for its callers.
async fn collect_ids(storage: &ContentStorage, prefix: Option<&str>) -> Vec<String> {
    let mut walk = storage.all_file_ids(prefix);
    let mut ids = Vec::new();
    while let Some(id) = walk.next().await.unwrap() {
        ids.push(id);
    }
    ids
}

#[tokio::test]
async fn store_retrieve_delete_roundtrip() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-{}", std::process::id()));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let data = Bytes::from_static(b"hello decentraland");

    storage.store(hash, data.clone()).await.unwrap();
    assert!(storage.exist(hash).await.unwrap());

    let retrieved = storage.retrieve(hash).await.unwrap().unwrap();
    assert_eq!(retrieved, data);

    let info = storage.file_info(hash).await.unwrap().unwrap();
    assert_eq!(info.size, data.len() as u64);
    assert!(info.encoding.is_none());

    storage.delete(hash).await.unwrap();
    assert!(!storage.exist(hash).await.unwrap());
    assert!(storage.retrieve(hash).await.unwrap().is_none());

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn exist_returns_false_for_missing() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-miss-{}", std::process::id()));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    assert!(!storage.exist(hash).await.unwrap());

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn delete_missing_is_silent() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-delmiss-{}", std::process::id()));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    storage.delete(hash).await.unwrap();

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn invalid_id_is_rejected() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-bad-{}", std::process::id()));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    match storage.exist("../etc/passwd").await {
        Err(StorageError::InvalidId(_)) => {}
        other => panic!("expected InvalidId, got {:?}", other),
    }
    match storage.store("Qm\0evil", Bytes::from_static(b"")).await {
        Err(StorageError::InvalidId(_)) => {}
        other => panic!("expected InvalidId, got {:?}", other),
    }

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn exist_multiple_invalid_id_is_a_per_id_miss() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-batch-{}", std::process::id()));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let stored = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let missing = "bafkreifzjut3te2nhyekklss27nh3k72ysco7y32koao5eei66wof36n5e";
    let over_long = format!("ba{}", "a".repeat(200));
    storage
        .store(stored, Bytes::from_static(b"x"))
        .await
        .unwrap();

    let results = storage
        .exist_multiple(&[over_long.as_str(), stored, "../etc/passwd", missing])
        .await
        .unwrap();
    assert_eq!(
        results,
        vec![
            (over_long.clone(), false),
            (stored.to_string(), true),
            ("../etc/passwd".to_string(), false),
            (missing.to_string(), false),
        ]
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn all_file_ids_lists_stored_files() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-list-{}", std::process::id()));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let a = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let b = "bafkreifzjut3te2nhyekklss27nh3k72ysco7y32koao5eei66wof36n5e";
    storage.store(a, Bytes::from_static(b"a")).await.unwrap();
    storage.store(b, Bytes::from_static(b"b")).await.unwrap();

    let ids = collect_ids(&storage, None).await;
    assert!(ids.contains(&a.to_string()));
    assert!(ids.contains(&b.to_string()));

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// The walk hands ids over as it finds them, so what it has not reached yet is not already held.
///
/// Deleting the second id after the first has been pulled is the only way a test can tell the two
/// shapes apart: a walk that materialized the corpus before returning would have captured the id
/// while it existed and would still offer it.
#[tokio::test]
async fn all_file_ids_streams_rather_than_buffering() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-stream-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let a = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let b = "bafkreifzjut3te2nhyekklss27nh3k72ysco7y32koao5eei66wof36n5e";
    assert_ne!(
        crate::hex_prefix(a),
        crate::hex_prefix(b),
        "the two ids must live in different shards for the walk to reach them separately"
    );
    storage.store(a, Bytes::from_static(b"a")).await.unwrap();
    storage.store(b, Bytes::from_static(b"b")).await.unwrap();

    let mut walk = storage.all_file_ids(None);
    let first = walk.next().await.unwrap().expect("one id is on disk");
    let second = if first == a { b } else { a };

    storage.delete_strict(second).await.unwrap();
    assert_eq!(
        walk.next().await.unwrap(),
        None,
        "an id deleted before the walk reached it is not yielded from a buffer"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// Enumeration is part of the read contract: a shard this walk listed was observed, so its later
/// removal is damage rather than "nothing was ever stored here".
#[tokio::test]
async fn enumeration_observes_the_shards_it_lists() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-enumobs-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let writer = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    writer.store(hash, Bytes::from_static(b"x")).await.unwrap();

    // A second instance that created nothing: enumeration is its only way to learn the shard.
    let reader = ContentStorage::new(&tmp).await.unwrap();
    assert_eq!(collect_ids(&reader, None).await, vec![hash.to_string()]);

    tokio::fs::remove_dir_all(shard_dir_of(&reader, hash))
        .await
        .unwrap();
    assert!(
        matches!(reader.exist(hash).await, Err(StorageError::Io(_))),
        "a shard the walk listed and then lost is damage, not an ordinary miss"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn retrieve_ignores_gzip_companion() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-gzip-{}", std::process::id()));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let raw_data = Bytes::from_static(b"raw content");
    let attacker_gzip = Bytes::from_static(b"attacker-planted gzip content");

    storage.store(hash, raw_data.clone()).await.unwrap();

    let raw_path = crate::resolve_file_path(storage.root(), hash).unwrap();
    let gzip_path = PathBuf::from(format!("{}.gzip", raw_path.display()));
    tokio::fs::write(&gzip_path, &attacker_gzip).await.unwrap();

    let retrieved = storage.retrieve(hash).await.unwrap().unwrap();
    assert_eq!(
        retrieved, raw_data,
        "retrieve() must NOT prefer a `.gzip` sibling (unverified)"
    );

    let info = storage.file_info(hash).await.unwrap().unwrap();
    assert!(info.encoding.is_none(), "file_info must report no encoding");
    assert_eq!(info.size, raw_data.len() as u64);

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[cfg(unix)]
fn running_as_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

use crate::wait_for_staging_cleanup;

fn shard_dir_of(storage: &ContentStorage, hash: &str) -> PathBuf {
    crate::resolve_file_path(storage.root(), hash)
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[cfg(unix)]
fn set_mode(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn read_probes_report_faults_not_misses() {
    if running_as_root() {
        eprintln!("skipping: permissions do not bind when running as root");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-fault-{}", std::process::id()));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    storage.store(hash, Bytes::from_static(b"x")).await.unwrap();

    let shard_dir = shard_dir_of(&storage, hash);
    set_mode(&shard_dir, 0o000);

    assert!(matches!(
        storage.exist(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.retrieve(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.retrieve_uncompressed(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.file_path(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.file_info(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.read_range(hash, 0, 0).await,
        Err(StorageError::Io(_))
    ));

    set_mode(&shard_dir, 0o755);
    assert!(storage.exist(hash).await.unwrap());

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn non_regular_file_at_content_path_is_a_fault() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-squat-{}", std::process::id()));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let path = crate::resolve_file_path(storage.root(), hash).unwrap();
    tokio::fs::create_dir_all(&path).await.unwrap();

    assert!(matches!(
        storage.exist(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.retrieve(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.file_path(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.file_info(hash).await,
        Err(StorageError::Io(_))
    ));

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn delete_strict_on_missing_is_ok() {
    let tmp = std::env::temp_dir().join(format!(
        "catalyrst-test-delstrict-miss-{}",
        std::process::id()
    ));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    storage.delete_strict(hash).await.unwrap();

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[cfg(unix)]
#[tokio::test]
async fn delete_strict_propagates_unlink_faults() {
    if running_as_root() {
        eprintln!("skipping: permissions do not bind when running as root");
        return;
    }
    let tmp = std::env::temp_dir().join(format!(
        "catalyrst-test-delstrict-fault-{}",
        std::process::id()
    ));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    storage.store(hash, Bytes::from_static(b"x")).await.unwrap();

    let shard_dir = shard_dir_of(&storage, hash);
    set_mode(&shard_dir, 0o555);

    assert!(matches!(
        storage.delete_strict(hash).await,
        Err(StorageError::Io(_))
    ));
    storage.delete(hash).await.unwrap();
    assert!(
        storage.exist(hash).await.unwrap(),
        "file must still be there"
    );

    set_mode(&shard_dir, 0o755);
    storage.delete_strict(hash).await.unwrap();
    assert!(!storage.exist(hash).await.unwrap());

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A shard nothing was ever stored in is an ordinary miss, on every read entry point.
#[tokio::test]
async fn read_of_never_created_shard_is_a_plain_miss() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-virgin-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    assert!(!storage.exist(hash).await.unwrap());
    assert!(storage.retrieve(hash).await.unwrap().is_none());
    assert!(storage.retrieve_uncompressed(hash).await.unwrap().is_none());
    assert!(storage.file_path(hash).await.unwrap().is_none());
    assert!(storage.file_info(hash).await.unwrap().is_none());
    assert!(storage.read_range(hash, 0, 31).await.unwrap().is_none());

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A range read serves the window it was asked for, clamped to the file it actually opened.
#[tokio::test]
async fn read_range_serves_only_the_window() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-range-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    storage
        .store(hash, Bytes::from_static(b"0123456789"))
        .await
        .unwrap();

    assert_eq!(
        storage.read_range(hash, 0, 3).await.unwrap().unwrap(),
        Bytes::from_static(b"0123")
    );
    assert_eq!(
        storage.read_range(hash, 4, 6).await.unwrap().unwrap(),
        Bytes::from_static(b"456"),
        "the window starts where it was asked to, not at the beginning of the file"
    );
    assert_eq!(
        storage.read_range(hash, 8, 4096).await.unwrap().unwrap(),
        Bytes::from_static(b"89"),
        "an end past the last byte is clamped to it"
    );
    assert_eq!(
        storage.read_range(hash, 9, 9).await.unwrap().unwrap(),
        Bytes::from_static(b"9")
    );
    assert!(
        storage.read_range(hash, 10, 20).await.unwrap().is_none(),
        "a window that starts past the end holds no bytes"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// Reads must have no filesystem side effects: the shard they probed stays absent.
#[tokio::test]
async fn reads_do_not_create_directories() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-nomkdir-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let shard_dir = shard_dir_of(&storage, hash);

    assert!(!storage.exist(hash).await.unwrap());
    let _ = storage.retrieve(hash).await.unwrap();
    let _ = storage.file_info(hash).await.unwrap();
    let _ = storage.file_path(hash).await.unwrap();
    storage.delete(hash).await.unwrap();
    storage.delete_strict(hash).await.unwrap();

    assert!(
        !shard_dir.exists(),
        "a read must not create {}",
        shard_dir.display()
    );
    let mut entries = tokio::fs::read_dir(storage.root()).await.unwrap();
    assert!(
        entries.next_entry().await.unwrap().is_none(),
        "reads must leave the storage root empty"
    );

    // The write path is still the thing that creates it.
    storage.store(hash, Bytes::from_static(b"x")).await.unwrap();
    assert!(shard_dir.is_dir());

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A shard destroyed underneath us is damage, not an empty node: every id in it must fault.
#[tokio::test]
async fn destroyed_shard_is_a_fault_not_a_miss() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-destroyed-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    // Any id in the destroyed shard qualifies: the fault is about the directory, not the file.
    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let shard_dir = shard_dir_of(&storage, hash);

    storage.store(hash, Bytes::from_static(b"x")).await.unwrap();
    tokio::fs::remove_dir_all(&shard_dir).await.unwrap();

    assert!(matches!(
        storage.exist(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.retrieve(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.retrieve_uncompressed(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.file_path(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.file_info(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.read_range(hash, 0, 3).await,
        Err(StorageError::Io(_))
    ));
    assert!(
        !shard_dir.exists(),
        "the faulting read must not have healed the shard"
    );

    // A write recreates the shard, and reads answer normally again.
    storage.store(hash, Bytes::from_static(b"y")).await.unwrap();
    assert!(storage.exist(hash).await.unwrap());

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// The damage report lasts as long as the damage. Asking once passes either way, which is why
/// this asks the same id three times: a report derived from an observation that the report itself
/// consumes makes the first read fault and every read after it answer "absent" over a completely
/// unchanged disk -- the silent data-loss answer, reached one read late.
#[tokio::test]
async fn a_reported_fault_is_stable_across_repeats() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-faultonce-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    storage.store(hash, Bytes::from_static(b"x")).await.unwrap();
    tokio::fs::remove_dir_all(shard_dir_of(&storage, hash))
        .await
        .unwrap();

    for attempt in 1..=3 {
        assert!(
            matches!(storage.exist(hash).await, Err(StorageError::Io(_))),
            "read {attempt} of a destroyed shard must still report the damage"
        );
    }
    assert!(matches!(
        storage.retrieve(hash).await,
        Err(StorageError::Io(_))
    ));

    // The store that repairs the shard ends the report, because the tree is whole again.
    storage.store(hash, Bytes::from_static(b"y")).await.unwrap();
    assert!(storage.exist(hash).await.unwrap());
    assert!(storage.exist(hash).await.unwrap());

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A storage root that is GONE is a destroyed store, whatever this instance has observed inside
/// it: `new()` created that directory, so nothing else has to prove it existed. Without the rule
/// a wiped volume answers "absent" for the entire key space -- one `rm -rf` read back as an empty
/// node by every consumer that syncs from it.
#[tokio::test]
async fn a_destroyed_root_is_a_fault_not_an_empty_node() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-noroot-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    // An id in a shard this instance never touched, so only the root can carry the answer.
    let never_stored = "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa";
    assert!(
        !storage.exist(never_stored).await.unwrap(),
        "with the root intact it is an ordinary miss"
    );

    tokio::fs::remove_dir_all(storage.root()).await.unwrap();
    assert!(
        matches!(storage.exist(never_stored).await, Err(StorageError::Io(_))),
        "a read must not report absence out of a root that is not there"
    );
    assert!(
        matches!(
            storage.retrieve(never_stored).await,
            Err(StorageError::Io(_))
        ),
        "and every read surface agrees"
    );
    assert!(
        matches!(
            storage.open_for_read(never_stored).await,
            Err(StorageError::Io(_))
        ),
        "including the streaming one"
    );
    assert!(
        storage.all_file_ids(None).next().await.is_err(),
        "enumeration must not answer 'this node holds nothing' out of a root that is gone"
    );
    assert!(
        !storage.root().exists(),
        "the faulting read must not have healed the root"
    );

    // A write recreates the tree, and reads answer normally again.
    storage
        .store(never_stored, Bytes::from_static(b"x"))
        .await
        .unwrap();
    assert!(storage.exist(never_stored).await.unwrap());

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// Why a root gets exactly ONE instance (see live/main.rs): the record of observed shards is
/// per-instance, so two instances over one root answer the same damage differently until each
/// has reported it. Sharing one `Arc` makes every consumer see one answer from the first read.
#[tokio::test]
async fn instances_over_one_root_agree_only_when_shared() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-twoinst-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";

    // Shared: the reader is the writer, so both surfaces answer identically.
    let shared = std::sync::Arc::new(ContentStorage::new(&tmp).await.unwrap());
    let reader = shared.clone();
    shared.store(hash, Bytes::from_static(b"x")).await.unwrap();
    tokio::fs::remove_dir_all(shard_dir_of(&shared, hash))
        .await
        .unwrap();
    assert!(
        matches!(reader.exist(hash).await, Err(StorageError::Io(_))),
        "a clone of the shared instance reports the same damage the writer would"
    );

    // Separate instances: the one that never observed the shard cannot know it was destroyed,
    // which is exactly the divergence the shared wiring exists to avoid.
    let writer = ContentStorage::new(&tmp).await.unwrap();
    let stranger = ContentStorage::new(&tmp).await.unwrap();
    writer.store(hash, Bytes::from_static(b"x")).await.unwrap();
    tokio::fs::remove_dir_all(shard_dir_of(&writer, hash))
        .await
        .unwrap();
    assert!(matches!(writer.exist(hash).await, Err(StorageError::Io(_))));
    assert!(
        !stranger.exist(hash).await.unwrap(),
        "documents the divergence: an instance that observed nothing reports a plain miss"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// `open_for_read` decides absence ONCE, so a shard destroyed under it is a fault: a caller that
/// stats and then opens has to invent an answer for an `ENOENT` its own stat said was impossible.
#[tokio::test]
async fn open_for_read_faults_on_a_destroyed_shard() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-openread-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let missing = "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa";

    assert!(
        storage.open_for_read(missing).await.unwrap().is_none(),
        "a never-created shard is a plain miss"
    );

    storage
        .store(hash, Bytes::from_static(b"hello"))
        .await
        .unwrap();
    let (_file, size) = storage.open_for_read(hash).await.unwrap().unwrap();
    assert_eq!(size, 5, "the size comes from the descriptor being returned");

    tokio::fs::remove_dir_all(shard_dir_of(&storage, hash))
        .await
        .unwrap();
    assert!(
        matches!(storage.open_for_read(hash).await, Err(StorageError::Io(_))),
        "a destroyed shard is a fault, not a 404"
    );

    // A directory at the content path faults before any body is streamed.
    let path = crate::resolve_file_path(storage.root(), hash).unwrap();
    tokio::fs::create_dir_all(&path).await.unwrap();
    assert!(matches!(
        storage.open_for_read(hash).await,
        Err(StorageError::Io(_))
    ));

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A FIFO at a content path must be rejected, not waited on.
///
/// `open(2)` O_RDONLY on a FIFO with no writer blocks until one shows up -- forever, here -- and
/// tokio runs it on the blocking pool, so without O_NONBLOCK every request for this id burned a
/// pool thread (cap 512) and the runtime could not shut down. The `stat`-based probes reject it
/// instantly, so `open_for_read` has to as well.
#[cfg(unix)]
#[tokio::test]
async fn a_fifo_at_a_content_path_is_rejected_not_awaited() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-fifo-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let path = crate::resolve_file_path(storage.root(), hash).unwrap();
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();

    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(
        unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) },
        0,
        "failed to create the test FIFO"
    );

    // No writer will ever open the other end. The timeout is the assertion: pre-fix this hung
    // until the test harness was killed.
    let verdict = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        storage.open_for_read(hash),
    )
    .await
    .expect("open_for_read blocked on a writer-less FIFO");
    assert!(
        matches!(verdict, Err(StorageError::Io(_))),
        "a FIFO is not content: it must be a fault"
    );

    // The stat-based probes agree, so no read surface disagrees about the same path.
    assert!(matches!(
        tokio::time::timeout(std::time::Duration::from_secs(5), storage.exist(hash))
            .await
            .expect("exist blocked on a writer-less FIFO"),
        Err(StorageError::Io(_))
    ));

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// Concurrent readers of one destroyed shard all report it. The answer comes from the tree, so
/// which reader asks first cannot decide what the others are told -- the alternative hands the
/// fault to one of them and a silent 404 to the other fifteen.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_destroyed_shard_is_reported_to_every_concurrent_reader() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-race-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = std::sync::Arc::new(ContentStorage::new(&tmp).await.unwrap());

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";

    // Repeated, because an answer that depends on interleaving can pass one round by luck.
    for _ in 0..25 {
        storage.store(hash, Bytes::from_static(b"x")).await.unwrap();
        tokio::fs::remove_dir_all(shard_dir_of(&storage, hash))
            .await
            .unwrap();

        let mut readers = Vec::new();
        for _ in 0..16 {
            let storage = storage.clone();
            readers.push(tokio::spawn(async move { storage.exist(hash).await }));
        }

        let mut faults = 0;
        for reader in readers {
            if reader.await.unwrap().is_err() {
                faults += 1;
            }
        }
        assert_eq!(
            faults, 16,
            "every reader of a destroyed shard must be told, not {faults} of them"
        );
    }

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A cancelled store leaves nothing behind. Axum drops a handler's future the moment the client
/// disconnects, and nothing else in the workspace reaps staging files.
#[tokio::test]
async fn cancelled_store_leaves_no_staging_file() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-cancel-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    // Big enough that the write is very likely still in flight when the timeout fires; if it
    // does complete, the assertion below holds for the committed path too.
    let payload = Bytes::from(vec![7u8; 64 * 1024 * 1024]);
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(1),
        storage.store(hash, payload),
    )
    .await;

    let leaked = wait_for_staging_cleanup(&shard_dir_of(&storage, hash)).await;
    assert!(
        leaked.is_empty(),
        "cancellation must not leak staging files, found {leaked:?}"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A file sitting in the wrong shard is unreachable by id, so enumeration must not offer it:
/// a consumer GC-ing from this list would act on a name `exist()` denies.
#[tokio::test]
async fn all_file_ids_skips_misplaced_and_non_content_entries() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-misplaced-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let stored = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let elsewhere = "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa";
    storage
        .store(stored, Bytes::from_static(b"a"))
        .await
        .unwrap();

    let shard = shard_dir_of(&storage, stored);
    // A canonical id, but in a shard its hash does not select.
    tokio::fs::write(shard.join(elsewhere), b"b").await.unwrap();
    // A leaked staging file and a stray directory.
    tokio::fs::write(shard.join(format!("{stored}.4242.0.tmp")), b"c")
        .await
        .unwrap();
    tokio::fs::create_dir(shard.join("junkdir")).await.unwrap();

    let ids = collect_ids(&storage, None).await;
    assert_eq!(
        ids,
        vec![stored.to_string()],
        "only the id that round-trips is yielded"
    );
    assert!(
        !storage.exist(elsewhere).await.unwrap(),
        "the misplaced file is indeed unreachable by id"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A stat that SUCCEEDS proves the shard intact, even when the path it found is unusable.
#[tokio::test]
async fn a_non_regular_file_still_teaches_the_shard() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-teaches-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let path = crate::resolve_file_path(storage.root(), hash).unwrap();
    tokio::fs::create_dir_all(&path).await.unwrap();

    // Faults because the path is a directory -- and records that the shard exists.
    assert!(matches!(
        storage.exist(hash).await,
        Err(StorageError::Io(_))
    ));

    tokio::fs::remove_dir_all(shard_dir_of(&storage, hash))
        .await
        .unwrap();
    assert!(
        matches!(storage.exist(hash).await, Err(StorageError::Io(_))),
        "the shard learned from that successful stat is missed when it disappears"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A read that never saw the shard learns it from the miss, so the NEXT destruction faults.
#[tokio::test]
async fn shard_observed_by_a_read_is_remembered() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-observed-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let writer = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let other = "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa";
    writer.store(hash, Bytes::from_static(b"x")).await.unwrap();

    // A second, read-only instance over the same tree: it created nothing, so its knowledge of
    // the shard can only come from having observed it.
    let reader = ContentStorage::new(&tmp).await.unwrap();
    assert!(reader.exist(hash).await.unwrap());
    // `other` hashes into a different shard, which this instance has never seen: still a miss.
    assert!(!reader.exist(other).await.unwrap());

    tokio::fs::remove_dir_all(shard_dir_of(&reader, hash))
        .await
        .unwrap();
    assert!(matches!(reader.exist(hash).await, Err(StorageError::Io(_))));
    assert!(!reader.exist(other).await.unwrap());

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A regular file squatting the shard path makes every id under it unreadable: a fault.
#[tokio::test]
async fn non_directory_at_shard_path_is_a_fault() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-shardfile-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
    let shard_dir = shard_dir_of(&storage, hash);
    tokio::fs::write(&shard_dir, b"not a directory")
        .await
        .unwrap();

    assert!(matches!(
        storage.exist(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        storage.retrieve(hash).await,
        Err(StorageError::Io(_))
    ));
    assert!(
        shard_dir.is_file(),
        "a foreign file at the shard path is never removed"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A symlink planted at the content path must not become a write-through to its target.
///
/// The staging path is the other half of this property, but its name carries a counter shared
/// with every other store in the process, so a test cannot predict it without becoming flaky;
/// that half is structural instead -- `create_new(true)` is `O_CREAT|O_EXCL`, which fails with
/// `EEXIST` on a symlink whatever it points at, and `O_NOFOLLOW` fails it a second way.
#[cfg(unix)]
#[tokio::test]
async fn store_does_not_write_through_a_planted_symlink() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-nofollow-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();
    let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";

    // A file OUTSIDE the storage root, and a symlink to it sitting where the content goes.
    let outside = tmp.join("outside-target");
    tokio::fs::write(&outside, b"do not touch").await.unwrap();
    let path = crate::resolve_file_path(storage.root(), hash).unwrap();
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    std::os::unix::fs::symlink(&outside, &path).unwrap();

    storage
        .store(hash, Bytes::from_static(b"new content"))
        .await
        .unwrap();

    assert_eq!(
        tokio::fs::read(&outside).await.unwrap(),
        b"do not touch",
        "the rename must REPLACE the symlink, never follow it to a file outside the root"
    );
    assert!(
        !tokio::fs::symlink_metadata(&path)
            .await
            .unwrap()
            .is_symlink(),
        "the content path is a real file afterwards"
    );
    assert_eq!(
        storage.retrieve(hash).await.unwrap().unwrap(),
        Bytes::from_static(b"new content")
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// A valid 59-char CIDv1 (`ba` + base32) whose two low digits encode `i`, so ids stay unique.
fn make_id(i: usize) -> String {
    const A: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut b = vec![b'a'; 59];
    b[0] = b'b';
    b[1] = b'a';
    b[2] = A[i % 32];
    b[3] = A[(i / 32) % 32];
    String::from_utf8(b).unwrap()
}

/// Applies the exact per-id error mapping `exist_multiple` uses, serially, for the reference.
async fn exist_ref(storage: &ContentStorage, id: &str) -> bool {
    match storage.exist(id).await {
        Ok(e) => e,
        Err(StorageError::InvalidId(_)) | Err(StorageError::PathTraversal(_)) => false,
        Err(e) => panic!("unexpected storage fault building reference: {e}"),
    }
}

#[tokio::test]
async fn exist_multiple_matches_serial_reference_order_and_semantics() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-test-existmany-{}", std::process::id()));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let good: Vec<String> = (0..40).map(make_id).collect();
    let stored = [3usize, 8, 14, 21, 37];
    for &i in &stored {
        storage
            .store(&good[i], Bytes::from_static(b"present"))
            .await
            .unwrap();
    }

    // Interleave two malformed ids among the good ones.
    let mut ids: Vec<String> = Vec::new();
    for (i, id) in good.iter().enumerate() {
        ids.push(id.clone());
        if i == 10 {
            ids.push("../evil".to_string());
        }
        if i == 25 {
            ids.push(String::new());
        }
    }

    // Reference: serial exist() with the same mapping, preserving input order.
    let mut reference: Vec<(String, bool)> = Vec::new();
    for id in &ids {
        reference.push((id.clone(), exist_ref(&storage, id).await));
    }

    let id_refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
    let got = storage.exist_multiple(&id_refs).await.unwrap();

    assert_eq!(
        got.len(),
        reference.len(),
        "length must match the serial walk"
    );
    assert_eq!(
        got, reference,
        "ids, order, and existence must match the serial walk"
    );

    // Both malformed ids read as (id, false) without aborting.
    assert!(got.contains(&("../evil".to_string(), false)));
    assert!(got.contains(&(String::new(), false)));
    // The 5 stored ids are true; a sampled un-stored good id is false.
    for &i in &stored {
        assert!(got.contains(&(good[i].clone(), true)));
    }
    assert!(got.contains(&(good[0].clone(), false)));

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[cfg(unix)]
#[tokio::test]
async fn exist_multiple_aborts_on_real_storage_fault() {
    // A real I/O fault must abort the batch, never read as `false`. Root ignores 0o000, so skip.
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("skipping: permissions do not bind when running as root");
        return;
    }
    use std::os::unix::fs::PermissionsExt;

    let tmp =
        std::env::temp_dir().join(format!("catalyrst-test-existfault-{}", std::process::id()));
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let poisoned = make_id(1);
    let healthy_missing = make_id(2);
    storage
        .store(&poisoned, Bytes::from_static(b"present"))
        .await
        .unwrap();

    let shard = storage.root().join(crate::hex_prefix(&poisoned));
    std::fs::set_permissions(&shard, std::fs::Permissions::from_mode(0o000)).unwrap();

    let res = storage
        .exist_multiple(&[healthy_missing.as_str(), poisoned.as_str()])
        .await;

    // Restore before asserting so cleanup can proceed regardless of the outcome.
    std::fs::set_permissions(&shard, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        res.is_err(),
        "a real storage fault must abort, not read as false"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

/// The exact shape of the 2026-05 snapshot incident: content sitting under a key it does not hash
/// to. `exist` is happy -- there is a file there -- and only re-reading the bytes tells them apart.
#[tokio::test]
async fn stored_content_hash_reports_the_bytes_not_the_key() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-verify-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    // Multi-level: past 174 leaves the DAG gains an interior layer, which is precisely where the
    // import's hasher went wrong and where a single-block implementation would agree by luck.
    let data = Bytes::from(
        (0..200u32 * 262_144)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<u8>>(),
    );
    let truth = catalyrst_hashing::hash_bytes_v1(&data);
    let wrong = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";

    storage.store(wrong, data.clone()).await.unwrap();

    assert!(storage.exist(wrong).await.unwrap(), "the file is present");
    assert_eq!(
        storage.stored_content_hash(wrong).await.unwrap().unwrap(),
        truth,
        "verification must report what the bytes hash to, not the key they were filed under"
    );

    assert!(storage.rekey(wrong, &truth).await.unwrap());
    assert!(
        !storage.exist(wrong).await.unwrap(),
        "the wrong key must stop serving; a 404 beats bytes under a CID they contradict"
    );
    assert_eq!(storage.retrieve(&truth).await.unwrap().unwrap(), data);
    assert_eq!(
        storage.stored_content_hash(&truth).await.unwrap().unwrap(),
        truth
    );

    assert!(
        !storage.rekey(wrong, &truth).await.unwrap(),
        "re-keying what is no longer there reports false rather than inventing a move"
    );

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn stored_content_hash_of_absent_id_is_none() {
    let tmp = std::env::temp_dir().join(format!("catalyrst-verify-abs-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let storage = ContentStorage::new(&tmp).await.unwrap();

    let absent = "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa";
    assert!(storage.stored_content_hash(absent).await.unwrap().is_none());

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}
