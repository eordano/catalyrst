//! Allocation-count perf guards for two hot paths:
//!   - feature_flags::snapshot() must be an O(1) Arc bump (item 1)
//!   - auth_api::get_identity must not deep-clone the identity JSON (item 2)
//!
//! Both tests share a counting global allocator and serialize against each other
//! so the per-test counter deltas are deterministic.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

use tower::ServiceExt;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
struct Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, SeqCst);
        System.alloc(l)
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, SeqCst);
        System.alloc_zeroed(l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, SeqCst);
        System.realloc(p, l, n)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
}
#[global_allocator]
static A: Counting = Counting;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn feature_flags_snapshot_is_o1() {
    let _g = SERIAL.lock().unwrap();
    std::env::remove_var("FEATURE_FLAGS_CONFIG_PATH");
    let fs = catalyrst_explorer_api::modules::feature_flags::FeatureFlagsState::default();
    let _warm = fs.snapshot();
    let before = ALLOCS.load(SeqCst);
    let snap = fs.snapshot();
    let delta = ALLOCS.load(SeqCst) - before;
    assert!(
        delta < 8,
        "snapshot() allocated {delta} times; expected O(1) Arc bump"
    );
    assert!(std::sync::Arc::ptr_eq(&snap, &fs.snapshot())); // only typechecks with Arc storage
}

#[test]
fn get_identity_does_not_clone_identity_json() {
    let _g = SERIAL.lock().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        use catalyrst_explorer_api::modules::auth_api::IdentityRecord;
        let cfg = catalyrst_explorer_api::config::Config::from_env().unwrap();
        let state = catalyrst_explorer_api::build_state(&cfg).await.unwrap();
        let app = catalyrst_explorer_api::api_router().with_state(state.clone());
        // many-node identity: clone cost = O(nodes), so alloc COUNT discriminates sharply
        let chain: Vec<serde_json::Value> = (0..4096)
            .map(|i| serde_json::json!({"type":"ECDSA_SIGNED_ENTITY","payload":format!("p-{i}"),"signature":format!("s-{i}")}))
            .collect();
        let id = "3fa85f64-5717-4562-b3fc-2c963f66afa6".to_string();
        let now = chrono::Utc::now();
        state.auth_api.identities.insert(
            id.clone(),
            IdentityRecord {
                identity_id: id.clone(),
                identity: serde_json::json!({ "authChain": chain }),
                ip_address: String::new(), // empty -> ip checks skipped
                is_mobile: false,
                created_at: now,
                expiration: now + chrono::Duration::seconds(600),
            },
        );
        let before = ALLOCS.load(SeqCst);
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri(format!("/auth/identities/{id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let delta = ALLOCS.load(SeqCst) - before;
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        assert!(
            delta < 6_000,
            "get_identity allocated {delta} times; the deep-clone path exceeds this"
        );
    });
}
