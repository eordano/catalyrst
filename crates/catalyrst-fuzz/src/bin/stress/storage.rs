use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::Barrier;

use catalyrst_hashing::hash_bytes_v1;
use catalyrst_storage::ContentStorage;

use crate::{fail, pass};

pub(crate) async fn test_content_storage_concurrent_writes() {
    println!("\n[2] Content storage concurrent writes");

    let tmp = std::env::temp_dir().join(format!("catalyrst-fuzz-storage-{}", std::process::id()));
    let storage = Arc::new(
        ContentStorage::new(&tmp)
            .await
            .expect("failed to create test storage"),
    );

    let barrier = Arc::new(Barrier::new(20));
    let mut handles = Vec::new();

    for i in 0..20u32 {
        let storage = storage.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            let data = Bytes::from(vec![(i & 0xff) as u8; 4096]);
            let hash = hash_bytes_v1(&data);
            barrier.wait().await;
            storage.store(&hash, data).await
        }));
    }

    let mut panics = 0;
    let mut errors = 0;
    for h in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                errors += 1;
                if errors == 1 {
                    fail("content_storage_writes_a", &format!("store error: {}", e));
                }
            }
            Err(e) => {
                panics += 1;
                if panics == 1 {
                    fail("content_storage_writes_a", &format!("task panicked: {}", e));
                }
            }
        }
    }

    if panics > 0 || errors > 0 {
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        return;
    }

    let mut read_wrong = 0;
    for i in 0..20u32 {
        let expected = Bytes::from(vec![(i & 0xff) as u8; 4096]);
        let hash = hash_bytes_v1(&expected);
        match storage.retrieve(&hash).await {
            Ok(Some(retrieved)) if retrieved == expected => {}
            Ok(Some(_)) => {
                read_wrong += 1;
            }
            Ok(None) => {
                read_wrong += 1;
            }
            Err(e) => {
                fail("content_storage_reads_a", &format!("retrieve error: {}", e));
            }
        }
    }

    if read_wrong > 0 {
        fail(
            "content_storage_reads_a",
            &format!("{}/20 reads returned wrong content", read_wrong),
        );
    } else {
        pass("20 concurrent writes (distinct hashes) + reads: all correct");
    }

    let single_data = Bytes::from(vec![0xABu8; 8192]);
    let single_hash = hash_bytes_v1(&single_data);
    storage
        .store(&single_hash, single_data.clone())
        .await
        .expect("failed to store single hash");

    let barrier2 = Arc::new(Barrier::new(20));
    let mut read_handles = Vec::new();
    for _ in 0..20 {
        let storage = storage.clone();
        let barrier2 = barrier2.clone();
        let hash = single_hash.to_string();
        read_handles.push(tokio::spawn(async move {
            barrier2.wait().await;
            storage.retrieve(&hash).await
        }));
    }

    let mut read_ok = 0;
    let mut wrong = 0;
    for h in read_handles {
        match h.await {
            Ok(Ok(Some(retrieved))) => {
                if retrieved == single_data {
                    read_ok += 1;
                } else {
                    wrong += 1;
                }
            }
            Ok(Ok(None)) => {
                wrong += 1;
            }
            Ok(Err(e)) => {
                fail("content_storage_reads_b", &format!("retrieve error: {}", e));
            }
            Err(e) => {
                fail("content_storage_reads_b", &format!("task panicked: {}", e));
            }
        }
    }

    if wrong > 0 {
        fail(
            "content_storage_reads_b",
            &format!("{}/20 concurrent reads got wrong content", wrong),
        );
    } else {
        pass(&format!(
            "1 write + 20 concurrent reads: all {} reads correct",
            read_ok
        ));
    }

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}
