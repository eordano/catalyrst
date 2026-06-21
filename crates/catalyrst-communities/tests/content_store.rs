use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use catalyrst_communities::content_store::{ContentError, ContentStore, MAX_BODY_BYTES};
use catalyrst_communities::fed::apply;
use catalyrst_communities::fed::messages::{CommunityCreate, CommunityPost};
use catalyrst_fed::sig::{domains, Eip712Domain};
use catalyrst_fed::{Signed, TypedMessage};
use ethers_signers::{LocalWallet, Signer};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

fn unique_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let mut rnd = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut rnd);
    p.push(format!("cmm-content-{}-{}", tag, hex::encode(rnd)));
    p
}

#[tokio::test]
async fn put_get_roundtrip_returns_sha256_hex() {
    let dir = unique_dir("rt");
    let store = ContentStore::new(&dir);
    store.init().await.unwrap();
    let body = b"# Hello\n\nThis is a community post body.";
    let h = store.put(body).await.unwrap();
    let mut hasher = Sha256::new();
    hasher.update(body);
    let expected = hex::encode(hasher.finalize());
    assert_eq!(h, expected);
    let got = store.get(&h).await.unwrap().expect("present");
    assert_eq!(got, body);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn put_twice_is_idempotent() {
    let dir = unique_dir("idem");
    let store = ContentStore::new(&dir);
    store.init().await.unwrap();
    let body = b"same body twice";
    let h1 = store.put(body).await.unwrap();
    let h2 = store.put(body).await.unwrap();
    assert_eq!(h1, h2);
    let bytes = store.get(&h1).await.unwrap().unwrap();
    assert_eq!(bytes, body);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn put_too_large_returns_too_large_error() {
    let dir = unique_dir("toobig");
    let store = ContentStore::new(&dir);
    store.init().await.unwrap();
    let body = vec![0u8; MAX_BODY_BYTES + 1];
    let err = store.put(&body).await.unwrap_err();
    assert!(matches!(err, ContentError::TooLarge { .. }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn get_missing_returns_none() {
    let dir = unique_dir("miss");
    let store = ContentStore::new(&dir);
    store.init().await.unwrap();
    let h = "0".repeat(64);
    assert!(store.get(&h).await.unwrap().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn put_expecting_rejects_mismatch() {
    let dir = unique_dir("mismatch");
    let store = ContentStore::new(&dir);
    store.init().await.unwrap();
    let body = b"truth";
    let wrong = "1".repeat(64);
    let err = store.put_expecting(body, &wrong).await.unwrap_err();
    assert!(matches!(err, ContentError::HashMismatch { .. }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn exists_is_fast_and_correct() {
    let dir = unique_dir("exists");
    let store = ContentStore::new(&dir);
    store.init().await.unwrap();
    let h = store.put(b"present").await.unwrap();
    assert!(store.exists(&h));
    assert!(!store.exists(&"f".repeat(64)));
    assert!(!store.exists("not-hex"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn gc_drops_unreferenced_keeps_referenced() {
    let dir = unique_dir("gc");
    let store = ContentStore::new(&dir);
    store.init().await.unwrap();
    let keep = store.put(b"keep me").await.unwrap();
    let drop1 = store.put(b"drop me").await.unwrap();
    let drop2 = store.put(b"drop me too").await.unwrap();
    assert!(store.exists(&keep));
    assert!(store.exists(&drop1));
    assert!(store.exists(&drop2));

    let mut refs: HashSet<String> = HashSet::new();
    refs.insert(keep.clone());
    let stats = store.gc(&refs).await.unwrap();
    assert_eq!(stats.kept, 1);
    assert_eq!(stats.removed, 2);
    assert!(store.exists(&keep));
    assert!(!store.exists(&drop1));
    assert!(!store.exists(&drop2));
    let _ = std::fs::remove_dir_all(&dir);
}

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_COMMUNITIES_TEST_PG")
        .ok()
        .or_else(|| Some("postgres://postgres:postgres@127.0.0.1:5432/communities".into()))
}

fn unique_schema() -> String {
    let mut b = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut b);
    format!("test_content_{}", hex::encode(b))
}

async fn setup_db() -> Option<(PgPool, String, String)> {
    let url = pg_url()?;
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .ok()?;
    let schema = unique_schema();
    sqlx::query(&format!("CREATE SCHEMA {}", schema))
        .execute(&admin)
        .await
        .ok()?;
    let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&suffixed)
        .await
        .ok()?;
    apply_migration(&pool, include_str!("../migrations/0001_initial.sql")).await;
    apply_migration(&pool, include_str!("../migrations/0002_federation.sql")).await;
    Some((pool, schema, url))
}

async fn apply_migration(pool: &PgPool, sql: &str) {
    let cleaned = strip_block_comments(sql);
    let mut buf = String::new();
    let mut in_func = false;
    for line in cleaned.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
        if trimmed.contains("$$ LANGUAGE plpgsql;") {
            in_func = false;
            sqlx::query(&buf)
                .execute(pool)
                .await
                .unwrap_or_else(|_| panic!("{}", buf.clone()));
            buf.clear();
            continue;
        }
        if trimmed.contains("CREATE OR REPLACE FUNCTION") || trimmed.contains("CREATE FUNCTION") {
            in_func = true;
        }
        if !in_func && trimmed.ends_with(';') {
            sqlx::query(&buf)
                .execute(pool)
                .await
                .unwrap_or_else(|_| panic!("{}", buf.clone()));
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        sqlx::query(&buf).execute(pool).await.expect("trailing sql");
    }
}

fn strip_block_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("--") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

async fn cleanup(admin_url: &str, schema: &str) {
    if let Ok(admin) = PgPoolOptions::new()
        .max_connections(1)
        .connect(admin_url)
        .await
    {
        let _ = sqlx::query(&format!("DROP SCHEMA {} CASCADE", schema))
            .execute(&admin)
            .await;
    }
}

fn mk_wallet(seed: u8) -> LocalWallet {
    let mut key = [0u8; 32];
    key[31] = seed;
    key[0] = 1;
    LocalWallet::from_bytes(&key).expect("wallet")
}

fn addr(w: &LocalWallet) -> String {
    format!("{:#x}", w.address())
}

fn rand_nonce() -> [u8; 16] {
    let mut n = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut n);
    n
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

async fn sign<T: TypedMessage>(
    wallet: &LocalWallet,
    message: T,
    domain: Eip712Domain,
    nonce: [u8; 16],
    signed_at: i64,
) -> Signed<T> {
    let mut signed = Signed {
        domain,
        message,
        nonce,
        signed_at,
        signature: String::new(),
    };
    let hash = signed.hash();
    let sig = wallet.sign_message(hash).await.unwrap();
    signed.signature = format!("0x{}", sig);
    signed
}

#[tokio::test]
async fn end_to_end_put_sign_post_get() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping end_to_end test: no postgres reachable");
        return;
    };
    let dir = unique_dir("e2e");
    let store = Arc::new(ContentStore::new(&dir));
    store.init().await.unwrap();

    let body = b"# Welcome to the community!\n\nThis is the first post body.";
    let content_hash = store.put(body).await.expect("put");

    let domain = domains::communities();
    let creator = mk_wallet(91);

    let create = sign(
        &creator,
        CommunityCreate {
            name: "ContentTest".into(),
            description: "".into(),
            private: false,
            unlisted: false,
            flags: vec![],
        },
        domain.clone(),
        rand_nonce(),
        now(),
    )
    .await;
    let applied = apply::apply_create(&pool, &create, &addr(&creator))
        .await
        .expect("create");
    let cid = applied.community_id.clone();

    let post = sign(
        &creator,
        CommunityPost {
            community_id: cid.clone(),
            content_hash: content_hash.clone(),
        },
        domain.clone(),
        rand_nonce(),
        now(),
    )
    .await;
    let sig = apply::apply_post(&pool, &post, &addr(&creator))
        .await
        .expect("apply_post");
    assert!(!sig.is_empty());

    assert!(store.exists(&content_hash));
    let got = store.get(&content_hash).await.unwrap().unwrap();
    assert_eq!(got, body);

    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT content_hash FROM community_posts_log WHERE signature_hash = $1")
            .bind(&sig)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, content_hash);

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn post_arrives_before_content_body_does_not_fail_apply() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping: no postgres reachable");
        return;
    };
    let dir = unique_dir("body-later");
    let store = Arc::new(ContentStore::new(&dir));
    store.init().await.unwrap();

    let body = b"body that arrives after the signed action";
    let mut hasher = Sha256::new();
    hasher.update(body);
    let content_hash = hex::encode(hasher.finalize());

    assert!(!store.exists(&content_hash));

    let domain = domains::communities();
    let creator = mk_wallet(92);
    let create = sign(
        &creator,
        CommunityCreate {
            name: "LaterBody".into(),
            description: "".into(),
            private: false,
            unlisted: false,
            flags: vec![],
        },
        domain.clone(),
        rand_nonce(),
        now(),
    )
    .await;
    let applied = apply::apply_create(&pool, &create, &addr(&creator))
        .await
        .unwrap();
    let post = sign(
        &creator,
        CommunityPost {
            community_id: applied.community_id.clone(),
            content_hash: content_hash.clone(),
        },
        domain.clone(),
        rand_nonce(),
        now(),
    )
    .await;
    apply::apply_post(&pool, &post, &addr(&creator))
        .await
        .expect("apply_post must succeed even when body is not yet local");

    let later = store.put(body).await.unwrap();
    assert_eq!(later, content_hash);
    assert!(store.exists(&content_hash));

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}
