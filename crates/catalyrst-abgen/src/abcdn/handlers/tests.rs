use super::jit_target;

#[test]
fn reason_header_helper_sets_taxonomy_values() {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    for reason in [
        "lod-not-built",
        "lod-jit-disabled:env-off",
        "lod-build-failed",
        "lod-build-failed-cached",
        "lod-build-inflight",
        "lod-build-timeout",
        "lod-level-unsupported",
        "bad-path",
    ] {
        let resp = super::with_reason((StatusCode::NOT_FOUND, "not found").into_response(), reason);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers()
                .get(super::lodjit::REASON_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some(reason)
        );
    }
    assert_eq!(
        super::lodjit::invalid_lod_reason("LOD/2/scene_2_windows"),
        "lod-level-unsupported"
    );
}

#[test]
fn is_ready_invariant_under_lod_jit_state() {
    use super::super::lodjit::LodJit;
    use std::path::PathBuf;

    let dir =
        std::env::temp_dir().join(format!("abgen-handlers-ready-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mk = |jit: LodJit| mk_lane_state_jit(&dir, None, None, "http://127.0.0.1:9", jit);

    use crate::lodgen::simplify::SimplifierBackend;
    let base = "/tmp/abgen-handlers-ready-test";
    let disabled = mk(LodJit::assemble(
        false,
        SimplifierBackend::Gltfpack,
        None,
        None,
        base,
        600,
        3600,
        1,
    ));
    let missing_dep = mk(LodJit::assemble(
        true,
        SimplifierBackend::Gltfpack,
        Some(Err(anyhow::anyhow!("gltfpack not found"))),
        None,
        base,
        600,
        3600,
        1,
    ));
    let enabled = mk(LodJit::assemble(
        true,
        SimplifierBackend::Gltfpack,
        Some(Ok(PathBuf::from("/bin/true"))),
        Some("/mb".to_string()),
        base,
        600,
        3600,
        1,
    ));
    assert!(!missing_dep.lod_jit.enabled);
    assert!(enabled.lod_jit.enabled);

    let r0 = super::is_ready(&disabled);
    assert_eq!(r0, super::is_ready(&missing_dep));
    assert_eq!(r0, super::is_ready(&enabled));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn metrics_authorized_gates_on_bearer_token() {
    use axum::http::HeaderMap;

    let empty = HeaderMap::new();
    assert!(super::metrics_authorized(None, &empty));
    assert!(!super::metrics_authorized(Some("s3cret"), &empty));

    let mut good = HeaderMap::new();
    good.insert("Authorization", "Bearer s3cret".parse().unwrap());
    assert!(super::metrics_authorized(Some("s3cret"), &good));
    assert!(super::metrics_authorized(None, &good));

    let mut wrong = HeaderMap::new();
    wrong.insert("Authorization", "Bearer nope".parse().unwrap());
    assert!(!super::metrics_authorized(Some("s3cret"), &wrong));

    let mut bare = HeaderMap::new();
    bare.insert("Authorization", "s3cret".parse().unwrap());
    assert!(!super::metrics_authorized(Some("s3cret"), &bare));
}

#[test]
fn pending_guard_decrements_on_drop_and_panic() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let pending = Arc::new(AtomicUsize::new(2));
    drop(super::PendingGuard(pending.clone()));
    assert_eq!(pending.load(Ordering::Relaxed), 1);

    let p2 = pending.clone();
    let unwound = std::panic::catch_unwind(move || {
        let _guard = super::PendingGuard(p2);
        panic!("boom");
    });
    assert!(unwound.is_err());
    assert_eq!(pending.load(Ordering::Relaxed), 0);
}

#[test]
fn jit_target_manifest_route() {
    let t = jit_target("manifest/bafkreiEntity_windows.json").unwrap();
    assert_eq!(t.entity(), "bafkreiEntity");
    assert_eq!(t.platform(), "windows");

    assert!(jit_target("manifest/bafkreiEntity_bogus.json").is_none());
    assert!(jit_target("manifest/noplatform.json").is_none());
}

#[test]
fn jit_target_skips_iss_manifest_route() {
    assert!(jit_target("lods-unity/manifests/bafkscene_InitialSceneState.json").is_none());
}

#[test]
fn materialize_tmp_paths_are_distinct_for_br_sidecars() {
    use std::path::Path;
    let a = super::materialize_tmp_path(Path::new("/out/LOD/1/a_windows"));
    let b = super::materialize_tmp_path(Path::new("/out/LOD/1/a_windows.br"));
    assert_ne!(a, b);
    let pid = std::process::id();
    let a = a.to_str().unwrap();
    let b = b.to_str().unwrap();
    assert!(
        a.starts_with(&format!("/out/LOD/1/a_windows.tmp.{pid}.")),
        "{a}"
    );
    assert!(
        b.starts_with(&format!("/out/LOD/1/a_windows.br.tmp.{pid}.")),
        "{b}"
    );
    assert_ne!(
        super::materialize_tmp_path(Path::new("/out/LOD/1/a_windows")),
        super::materialize_tmp_path(Path::new("/out/LOD/1/a_windows"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dispatch_local_serves_iss_manifest_route() {
    use axum::http::{HeaderMap, Method, StatusCode};

    let dir = std::env::temp_dir().join(format!("abgen-handlers-iss-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sid = "bafkscene";
    std::fs::create_dir_all(dir.join(sid)).unwrap();
    let body = r#"{"version":1,"sceneId":"bafkscene","assets":[]}"#;
    std::fs::write(
        dir.join(sid).join(format!("{sid}_InitialSceneState.json")),
        body,
    )
    .unwrap();

    let state = mk_lane_state(&dir, None);
    let headers = HeaderMap::new();

    let path = format!("lods-unity/manifests/{sid}_InitialSceneState.json");
    let resp = super::dispatch_local(&state, &path, &Method::GET, &headers).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok()),
        Some("application/json")
    );
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|v| v.to_str().ok()),
        Some("private, max-age=0, no-cache")
    );
    let got = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&got[..], body.as_bytes());

    let head = super::dispatch_local(&state, &path, &Method::HEAD, &headers).await;
    assert_eq!(head.status(), StatusCode::OK);

    let missing = super::dispatch_local(
        &state,
        "lods-unity/manifests/bafkunknown_InitialSceneState.json",
        &Method::GET,
        &headers,
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let bad = super::dispatch_local(
        &state,
        "lods-unity/manifests/bafkscene_lod.json",
        &Method::GET,
        &headers,
    )
    .await;
    assert_eq!(bad.status(), StatusCode::NOT_FOUND);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn jit_target_bundle_route() {
    let t = jit_target("v41/bafkEntity/Qmhash_mac").unwrap();
    assert_eq!(t.entity(), "bafkEntity");
    assert_eq!(t.platform(), "mac");

    assert!(jit_target("v41/bafkEntity/Qmhash_linux.br").is_none());
    assert!(super::br_bundle_target("v41/bafkEntity/Qmhash_linux.br"));
    assert!(!super::br_bundle_target("v41/bafkEntity/Qmhash_linux"));
    assert!(!super::br_bundle_target("v41/Qmhash_windows.br"));
    assert!(!super::br_bundle_target(
        "manifest/bafkEntity_windows.json.br"
    ));
}

#[test]
fn jit_target_skips_native_and_flat() {
    assert!(jit_target("v41/bafkEntity/scene.json").is_none());
    assert!(jit_target("v41/bafkEntity/main.crdt").is_none());

    assert!(jit_target("v41/Qmhash_windows").is_none());

    assert!(jit_target("LOD/2/scene_2_windows").is_none());
}

#[test]
fn flat_target_table() {
    assert_eq!(
        super::flat_target("v41/Qmhash_windows"),
        Some(("Qmhash".to_string(), "windows".to_string()))
    );
    assert_eq!(
        super::flat_target("v41/Qmhash_mac.br"),
        Some(("Qmhash".to_string(), "mac".to_string()))
    );
    assert_eq!(
        super::flat_target("v41/Qmhash_webgl"),
        Some(("Qmhash".to_string(), "webgl".to_string()))
    );
    assert_eq!(super::flat_target("v41/Qmhash"), None);
    assert_eq!(super::flat_target("v41/_windows"), None);
    assert_eq!(super::flat_target("manifest/Qmhash_windows"), None);
    assert_eq!(super::flat_target("LOD/Qmhash_windows"), None);
    assert_eq!(super::flat_target("lods-unity/manifests"), None);
    assert_eq!(super::flat_target("dcl/scene_ignore_windows"), None);
    assert_eq!(super::flat_target("v41/a/b"), None);
    assert_eq!(super::flat_target("../Qmhash_windows"), None);
}

#[test]
fn world_name_validation() {
    assert!(super::valid_world_name("foo.dcl.eth"));
    assert!(super::valid_world_name("my-world_2"));
    assert!(!super::valid_world_name(".."));
    assert!(!super::valid_world_name("a/b"));
    assert!(!super::valid_world_name("a b"));
    assert!(!super::valid_world_name("a?b"));
    assert!(!super::valid_world_name(""));
}

fn lane_temp_dir(tag: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("abgen-handlers-lane-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mk_lane_state(
    dir: &std::path::Path,
    proxy: Option<std::sync::Arc<crate::live::Proxy>>,
) -> super::super::state::AppState {
    mk_lane_state_worlds(dir, proxy, None)
}

fn mk_lane_state_worlds(
    dir: &std::path::Path,
    proxy: Option<std::sync::Arc<crate::live::Proxy>>,
    worlds_content_url: Option<String>,
) -> super::super::state::AppState {
    mk_lane_state_content(dir, proxy, worlds_content_url, "http://127.0.0.1:9")
}

fn mk_lane_state_content(
    dir: &std::path::Path,
    proxy: Option<std::sync::Arc<crate::live::Proxy>>,
    worlds_content_url: Option<String>,
    content_url: &str,
) -> super::super::state::AppState {
    use super::super::lodjit::LodJit;
    mk_lane_state_jit(
        dir,
        proxy,
        worlds_content_url,
        content_url,
        LodJit::assemble(
            false,
            crate::lodgen::simplify::SimplifierBackend::Gltfpack,
            None,
            None,
            "/tmp/abgen-handlers-lane",
            600,
            3600,
            1,
        ),
    )
}

fn mk_lane_state_jit(
    dir: &std::path::Path,
    proxy: Option<std::sync::Arc<crate::live::Proxy>>,
    worlds_content_url: Option<String>,
    content_url: &str,
    jit: super::super::lodjit::LodJit,
) -> super::super::state::AppState {
    use super::super::state::AppStateInner;
    std::sync::Arc::new(
        AppStateInner::new(
            dir.to_path_buf(),
            crate::catalyst::CatalystClient::new(content_url),
            std::collections::HashMap::new(),
            proxy,
            "http://c".to_string(),
            true,
            Vec::new(),
            "v41".to_string(),
            "date".to_string(),
            "http://c".to_string(),
            true,
            jit,
            crate::abcdn::state::IndexBuild::disabled(),
        )
        .with_worlds_content_url(worlds_content_url),
    )
}

fn mk_stub_proxy(host: &str, read_only: bool, tag: &str) -> std::sync::Arc<crate::live::Proxy> {
    mk_stub_proxy_catalyst(host, "http://127.0.0.1:9", read_only, tag)
}

fn mk_stub_proxy_catalyst(
    host: &str,
    catalyst_url: &str,
    read_only: bool,
    tag: &str,
) -> std::sync::Arc<crate::live::Proxy> {
    crate::live::stub::stub_proxy_at(
        host,
        catalyst_url,
        read_only,
        &lane_temp_dir(&format!("{tag}-cache")),
    )
}

async fn lane_get(state: &super::super::state::AppState, path: &str) -> axum::response::Response {
    use axum::extract::State;
    super::dispatch(
        State(state.clone()),
        axum::http::Method::GET,
        axum::http::HeaderMap::new(),
        format!("/{path}").parse().unwrap(),
    )
    .await
}

async fn body_bytes(resp: axum::response::Response) -> Vec<u8> {
    axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

fn reason_of(resp: &axum::response::Response) -> Option<String> {
    resp.headers()
        .get(super::lodjit::REASON_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lod_lane_reads_through_space_before_jit() {
    use axum::http::StatusCode;
    let dir = lane_temp_dir("lodrt");
    let sid = "bafklodrt";
    let (host, seen) = crate::live::stub::serve(vec![(
        format!("/LOD/1/{sid}_1_windows"),
        200,
        b"LODBYTES".to_vec(),
    )]);
    let state = mk_lane_state(&dir, Some(mk_stub_proxy(&host, false, "lodrt")));

    let resp = lane_get(&state, &format!("LOD/1/{sid}_1_windows")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("ETag").and_then(|v| v.to_str().ok()),
        Some(format!("\"{sid}_1_windows\"").as_str())
    );
    assert_eq!(body_bytes(resp).await, b"LODBYTES");
    assert!(dir
        .join(sid)
        .join("LOD")
        .join("1")
        .join(format!("{sid}_1_windows"))
        .is_file());
    assert!(seen
        .lock()
        .unwrap()
        .contains(&format!("GET /LOD/1/{sid}_1_windows")));

    let miss = lane_get(&state, "LOD/1/bafkother_1_windows").await;
    assert_eq!(miss.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        reason_of(&miss).as_deref(),
        Some("lod-jit-disabled:env-off")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iss_lane_reads_through_space_and_reports_reason() {
    use axum::http::StatusCode;
    let dir = lane_temp_dir("issrt");
    let sid = "bafkissrt";
    let body = br#"{"version":1,"sceneId":"bafkissrt","assets":[]}"#.to_vec();
    let (host, _seen) = crate::live::stub::serve(vec![(
        format!("/lods-unity/manifests/{sid}_InitialSceneState.json"),
        200,
        body.clone(),
    )]);
    let state = mk_lane_state(&dir, Some(mk_stub_proxy(&host, false, "issrt")));

    let path = format!("lods-unity/manifests/{sid}_InitialSceneState.json");
    let resp = lane_get(&state, &path).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok()),
        Some("application/json")
    );
    assert_eq!(body_bytes(resp).await, body);
    assert!(dir
        .join(sid)
        .join(format!("{sid}_InitialSceneState.json"))
        .is_file());

    let miss = lane_get(
        &state,
        "lods-unity/manifests/bafkmissing_InitialSceneState.json",
    )
    .await;
    assert_eq!(miss.status(), StatusCode::NOT_FOUND);
    assert_eq!(reason_of(&miss).as_deref(), Some("iss-not-built"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lod_jit_success_writes_back_to_space() {
    let dir = lane_temp_dir("lodwb");
    let sid = "bafklodwb";
    let (host, seen) = crate::live::stub::serve(vec![]);
    let state = mk_lane_state(&dir, Some(mk_stub_proxy(&host, false, "lodwb")));
    let ldir = dir.join(sid).join("LOD").join("1");
    std::fs::create_dir_all(&ldir).unwrap();
    for plat in ["windows", "mac", "linux"] {
        std::fs::write(ldir.join(format!("{sid}_1_{plat}")), b"bundle").unwrap();
    }
    let ldir0 = dir.join(sid).join("LOD").join("0");
    std::fs::create_dir_all(&ldir0).unwrap();
    std::fs::write(ldir0.join(format!("{sid}_0_windows")), b"bundle0").unwrap();
    std::fs::write(
        dir.join(sid).join(format!("{sid}_InitialSceneState.json")),
        b"{}",
    )
    .unwrap();

    super::spawn_lod_writeback(&state, sid);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let log = seen.lock().unwrap().clone();
        let puts: Vec<&String> = log.iter().filter(|l| l.starts_with("PUT ")).collect();
        if puts.len() >= 5 {
            for plat in ["windows", "mac", "linux"] {
                assert!(
                    log.contains(&format!("PUT /LOD/1/{sid}_1_{plat}")),
                    "{log:?}"
                );
            }
            assert!(
                log.contains(&format!("PUT /LOD/0/{sid}_0_windows")),
                "{log:?}"
            );
            assert!(
                log.contains(&format!(
                    "PUT /lods-unity/manifests/{sid}_InitialSceneState.json"
                )),
                "{log:?}"
            );
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "write-back never completed: {log:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let (host_ro, seen_ro) = crate::live::stub::serve(vec![]);
    let state_ro = mk_lane_state(&dir, Some(mk_stub_proxy(&host_ro, true, "lodwb-ro")));
    super::spawn_lod_writeback(&state_ro, sid);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(seen_ro.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flat_lane_space_mirror_and_alias_rewrite() {
    use axum::http::StatusCode;
    let dir = lane_temp_dir("flat");
    let cid = "bafkflatowner";
    let hash = "Qmflathash";
    let (host, _seen) = crate::live::stub::serve(vec![
        (
            "/v41/Qmflatmirror_windows".to_string(),
            200,
            b"MIRROR".to_vec(),
        ),
        (format!("/v41/{cid}/{hash}_windows"), 200, b"ALIAS".to_vec()),
    ]);
    let proxy = mk_stub_proxy(&host, false, "flat");
    let state = mk_lane_state(&dir, Some(proxy.clone()));

    let resp = lane_get(&state, "v41/Qmflatmirror_windows").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("ETag").and_then(|v| v.to_str().ok()),
        Some("\"Qmflatmirror_windows\"")
    );
    assert_eq!(body_bytes(resp).await, b"MIRROR");
    assert!(dir.join("Qmflatmirror_windows").is_file());

    proxy.index_content_hashes(vec![(hash.to_string(), cid.to_string())]);
    let resp = lane_get(&state, &format!("v41/{hash}_windows")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("ETag").and_then(|v| v.to_str().ok()),
        Some(format!("\"{hash}_windows\"").as_str())
    );
    assert_eq!(body_bytes(resp).await, b"ALIAS");
    let nested = dir
        .join(cid)
        .join("windows")
        .join(format!("{hash}_windows"));
    assert!(nested.is_file());

    let local = super::dispatch_local(
        &state,
        &format!("v41/{cid}/{hash}_windows"),
        &axum::http::Method::GET,
        &axum::http::HeaderMap::new(),
    )
    .await;
    assert_eq!(local.status(), StatusCode::OK);
    assert_eq!(
        local.headers().get("ETag").and_then(|v| v.to_str().ok()),
        Some(format!("\"{hash}_windows\"").as_str())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flat_lane_unresolvable_hash_gets_reason_and_negcache() {
    use axum::http::StatusCode;
    let dir = lane_temp_dir("flatneg");
    let (host, _seen) = crate::live::stub::serve(vec![]);
    let (chost, cseen) = crate::live::stub::serve(vec![]);
    let state = mk_lane_state_content(
        &dir,
        Some(mk_stub_proxy(&host, false, "flatneg")),
        None,
        &format!("http://{chost}"),
    );

    let resp = lane_get(&state, "v41/Qmunknownhash_windows").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(reason_of(&resp).as_deref(), Some("hash-unresolved"));
    assert!(state.hash_neg_cache.get("Qmunknownhash").await.is_some());
    assert!(state.hash_neg_cache.get("qmunknownhash").await.is_none());
    let lookups_before = cseen.lock().unwrap().len();

    let again = lane_get(&state, "v41/Qmunknownhash_windows").await;
    assert_eq!(again.status(), StatusCode::NOT_FOUND);
    assert_eq!(reason_of(&again).as_deref(), Some("hash-unresolved"));
    assert_eq!(cseen.lock().unwrap().len(), lookups_before);

    let other_platform = lane_get(&state, "v41/Qmunknownhash_mac").await;
    assert_eq!(
        reason_of(&other_platform).as_deref(),
        Some("hash-unresolved")
    );
    assert_eq!(cseen.lock().unwrap().len(), lookups_before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flat_lane_resolver_error_is_not_negcached() {
    use axum::http::StatusCode;
    let dir = lane_temp_dir("flaterr");
    let (host, _seen) = crate::live::stub::serve(vec![]);
    let state = mk_lane_state(&dir, Some(mk_stub_proxy(&host, false, "flaterr")));

    let resp = lane_get(&state, "v41/Qmerrhash_windows").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(reason_of(&resp).as_deref(), Some("hash-unresolved"));
    state.hash_neg_cache.run_pending_tasks().await;
    assert!(state.hash_neg_cache.get("Qmerrhash").await.is_none());
    assert_eq!(state.hash_neg_cache.entry_count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flat_lane_wrong_case_negcache_does_not_poison_exact_hash() {
    use axum::http::StatusCode;
    let dir = lane_temp_dir("flatcase");
    let (host, _seen) = crate::live::stub::serve(vec![]);
    let (chost, _cseen) = crate::live::stub::serve(vec![]);
    let proxy = mk_stub_proxy_catalyst(&host, &format!("http://{chost}"), false, "flatcase");
    let state = mk_lane_state_content(&dir, Some(proxy.clone()), None, &format!("http://{chost}"));

    let wrong = lane_get(&state, "v41/qmpoison_windows").await;
    assert_eq!(wrong.status(), StatusCode::NOT_FOUND);
    assert_eq!(reason_of(&wrong).as_deref(), Some("hash-unresolved"));
    assert!(state.hash_neg_cache.get("qmpoison").await.is_some());

    proxy.index_content_hashes(vec![("QmPoison".to_string(), "bafkowner".to_string())]);
    let right = lane_get(&state, "v41/QmPoison_windows").await;
    assert_eq!(right.status(), StatusCode::NOT_FOUND);
    assert_ne!(reason_of(&right).as_deref(), Some("hash-unresolved"));
    assert!(state.hash_neg_cache.get("QmPoison").await.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn jit_single_flight_coalesces_and_negcaches_failures() {
    let dir = lane_temp_dir("jitsf");
    let (host, _seen) = crate::live::stub::serve(vec![]);
    let (chost, cseen) = crate::live::stub::serve(vec![]);
    let proxy = mk_stub_proxy_catalyst(&host, &format!("http://{chost}"), false, "jitsf");
    let state = mk_lane_state_content(&dir, Some(proxy.clone()), None, "http://127.0.0.1:9");

    let probe = dir.join("bafkE").join("windows").join("Qmx_windows");
    std::fs::create_dir_all(probe.parent().unwrap()).unwrap();
    std::fs::write(&probe, b"AB").unwrap();
    let got =
        super::jit_build_entity(&state, &proxy, "bafkE", "windows", Some(probe), "entity").await;
    assert!(matches!(got, super::JitBuild::Coalesced));
    assert!(cseen.lock().unwrap().is_empty());

    let missing = dir.join("nope");
    let got = super::jit_build_entity(
        &state,
        &proxy,
        "bafkMiss",
        "windows",
        Some(missing.clone()),
        "entity",
    )
    .await;
    assert!(matches!(got, super::JitBuild::Failed));
    assert!(state.jit_fail_cache.get("bafkMiss:windows").await.is_some());
    let calls_before = cseen.lock().unwrap().len();

    let again = super::jit_build_entity(
        &state,
        &proxy,
        "bafkMiss",
        "windows",
        Some(missing),
        "entity",
    )
    .await;
    assert!(matches!(again, super::JitBuild::Failed));
    assert_eq!(cseen.lock().unwrap().len(), calls_before);
    assert!(state.jit_inflight.lock().await.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn br_bundle_requests_skip_the_jit_build() {
    use axum::http::StatusCode;
    let dir = lane_temp_dir("brskip");
    let (host, _seen) = crate::live::stub::serve(vec![]);
    let (chost, cseen) = crate::live::stub::serve(vec![]);
    let proxy = mk_stub_proxy_catalyst(&host, &format!("http://{chost}"), false, "brskip");
    let state = mk_lane_state_content(&dir, Some(proxy), None, &format!("http://{chost}"));

    let resp = lane_get(&state, "v41/bafkEnt/Qmhash_windows.br").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(reason_of(&resp).as_deref(), Some("br-not-built"));
    assert!(cseen.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn if_none_match_star_needs_an_existing_file() {
    use axum::http::{HeaderMap, Method, StatusCode};
    let dir = lane_temp_dir("inm");
    let state = mk_lane_state(&dir, None);
    let mut headers = HeaderMap::new();
    headers.insert("if-none-match", "*".parse().unwrap());
    let path = "v41/bafkEntity/Qmhash_windows";

    let missing = super::dispatch_local(&state, path, &Method::GET, &headers).await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let nested = dir.join("bafkEntity").join("windows");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("Qmhash_windows"), b"AB").unwrap();
    state.resolve_cache.invalidate(path).await;
    let present = super::dispatch_local(&state, path, &Method::GET, &headers).await;
    assert_eq!(present.status(), StatusCode::NOT_MODIFIED);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shader_lane_materializes_vendored_and_strips_scene_id() {
    use axum::http::StatusCode;
    if !crate::shader::vendored_path().exists() {
        eprintln!("vendored shader bundle missing, skipping");
        return;
    }
    let dir = lane_temp_dir("shader");
    let state = mk_lane_state(&dir, None);
    let expect = crate::shader::bundle_bytes_verified().unwrap();

    let plain = lane_get(&state, "v41/dcl/scene_ignore_windows").await;
    assert_eq!(plain.status(), StatusCode::OK);
    assert_eq!(
        plain.headers().get("ETag").and_then(|v| v.to_str().ok()),
        Some("\"scene_ignore_windows\"")
    );
    let plain_body = body_bytes(plain).await;
    assert_eq!(plain_body, expect);
    assert!(dir.join("dcl").join("scene_ignore_windows").is_file());
    assert!(dir.join("scene_ignore_windows").is_file());

    let scoped = lane_get(&state, "v41/bafkscene123/dcl/scene_ignore_windows").await;
    assert_eq!(scoped.status(), StatusCode::OK);
    let scoped_body = body_bytes(scoped).await;
    assert_eq!(
        crate::hashes::sha256_hex(&scoped_body),
        crate::hashes::sha256_hex(&plain_body)
    );

    let lit = lane_get(
        &state,
        "v41/dcl/universal%20render%20pipeline/lit_ignore_windows",
    )
    .await;
    assert_eq!(lit.status(), StatusCode::NOT_FOUND);
    assert_eq!(reason_of(&lit).as_deref(), Some("shader-unavailable"));

    if crate::shader::vendored_path_named("scene_ignore_mac").exists() {
        let mac = lane_get(&state, "v41/bafkscene123/dcl/scene_ignore_mac").await;
        assert_eq!(mac.status(), StatusCode::OK);
        assert_eq!(
            crate::hashes::sha256_hex(&body_bytes(mac).await),
            crate::shader::vendored_sha("scene_ignore_mac").unwrap()
        );
        assert!(dir.join("dcl").join("scene_ignore_mac").is_file());
        assert!(dir.join("scene_ignore_mac").is_file());
    } else {
        eprintln!("vendored shader bundle missing, skipping mac lane");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shader_lane_mac_vendored_writes_back_to_space() {
    use axum::http::StatusCode;
    if !crate::shader::vendored_path_named("scene_ignore_mac").exists() {
        eprintln!("vendored shader bundle missing, skipping");
        return;
    }
    let dir = lane_temp_dir("shadermac");
    let (host, seen) = crate::live::stub::serve(vec![]);
    let state = mk_lane_state(&dir, Some(mk_stub_proxy(&host, false, "shadermac")));

    let resp = lane_get(&state, "v41/bafkscene123/dcl/scene_ignore_mac").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        crate::hashes::sha256_hex(&body_bytes(resp).await),
        crate::shader::vendored_sha("scene_ignore_mac").unwrap()
    );
    assert!(dir.join("dcl").join("scene_ignore_mac").is_file());
    assert!(dir.join("scene_ignore_mac").is_file());
    let log = seen.lock().unwrap().clone();
    assert!(
        log.iter().any(|l| l == "PUT /v41/dcl/scene_ignore_mac"),
        "{log:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shader_lane_reads_lit_payload_from_space() {
    use axum::http::StatusCode;
    let dir = lane_temp_dir("shaderlit");
    let (host, seen) = crate::live::stub::serve(vec![(
        "/v41/dcl/universal%20render%20pipeline/lit_ignore_windows".to_string(),
        200,
        b"LITBYTES".to_vec(),
    )]);
    let state = mk_lane_state(&dir, Some(mk_stub_proxy(&host, false, "shaderlit")));

    let resp = lane_get(
        &state,
        "v41/bafkscene/dcl/universal%20render%20pipeline/lit_ignore_windows",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("ETag").and_then(|v| v.to_str().ok()),
        Some("\"lit_ignore_windows\"")
    );
    assert_eq!(body_bytes(resp).await, b"LITBYTES");
    assert!(dir
        .join("dcl")
        .join("universal render pipeline")
        .join("lit_ignore_windows")
        .is_file());
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .any(|l| l.contains("/v41/dcl/universal%20render%20pipeline/lit_ignore_windows")));

    let native = lane_get(&state, "v41/bafkEntity/some/nested/file.bin").await;
    assert_eq!(native.status(), StatusCode::NOT_FOUND);
    assert_eq!(reason_of(&native), None);
    let _ = std::fs::remove_dir_all(&dir);
}

static PRIVATE_BASE_URL_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn lock_private_base_url_env() -> tokio::sync::MutexGuard<'static, ()> {
    PRIVATE_BASE_URL_ENV.lock().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entities_active_honors_world_name_param() {
    use axum::extract::{Query, State};
    use axum::http::StatusCode;
    let _env = lock_private_base_url_env().await;
    let dir = lane_temp_dir("worlds");
    let cid = "bafkworldscene";
    std::env::set_var("ABGEN_ALLOW_PRIVATE_BASE_URL", "1");

    let entity = serde_json::json!({
        "id": cid,
        "type": "scene",
        "timestamp": 1234567i64,
        "pointers": ["0,0"],
        "content": [{"file": "model.glb", "hash": "Qmworldglb"}],
        "metadata": {"worldConfiguration": {"name": "test.dcl.eth"}},
    });
    let (chost, _cseen) = crate::live::stub::serve(vec![(
        format!("/contents/{cid}"),
        200,
        entity.to_string().into_bytes(),
    )]);
    let about = serde_json::json!({
        "configurations": {
            "scenesUrn": [format!(
                "urn:decentraland:entity:{cid}?=&baseUrl=http://{chost}/contents/"
            )]
        }
    });
    let (whost, _wseen) = crate::live::stub::serve(vec![(
        "/world/test.dcl.eth/about".to_string(),
        200,
        about.to_string().into_bytes(),
    )]);

    let state = mk_lane_state_worlds(&dir, None, Some(format!("http://{whost}")));

    let mut q = std::collections::HashMap::new();
    q.insert("world_name".to_string(), "test.dcl.eth".to_string());
    let resp = super::post_entities_active(
        State(state.clone()),
        Query(q),
        axum::response::Json(serde_json::json!({"pointers": ["0,0"]})),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(&body_bytes(resp).await).unwrap();
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    let rec = &arr[0];
    assert_eq!(rec["id"], cid);
    assert_eq!(rec["type"], "scene");
    assert_eq!(rec["timestamp"], 1234567);
    assert_eq!(rec["pointers"], serde_json::json!(["0,0"]));
    assert_eq!(rec["content"][0]["hash"], "Qmworldglb");
    assert_eq!(rec["versions"]["assets"]["windows"]["version"], "v41");
    assert_eq!(rec["deployer"], "");

    let plain = super::post_entities_active(
        State(state.clone()),
        Query(std::collections::HashMap::new()),
        axum::response::Json(serde_json::json!({"pointers": ["0,0"]})),
    )
    .await;
    assert_eq!(plain.status(), StatusCode::OK);
    let plain_body: serde_json::Value = serde_json::from_slice(&body_bytes(plain).await).unwrap();
    assert_eq!(plain_body, serde_json::json!([]));

    let bad = super::post_entities_active(
        State(state.clone()),
        Query(std::collections::HashMap::new()),
        axum::response::Json(serde_json::json!({"pointers": []})),
    )
    .await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    std::env::remove_var("ABGEN_ALLOW_PRIVATE_BASE_URL");
    let _ = std::fs::remove_dir_all(&dir);
}

fn mk_registry_state(
    dir: &std::path::Path,
    catalyst: &str,
    worlds: Option<String>,
) -> dcl_contents::registry::RegistryAppState {
    std::sync::Arc::new(dcl_contents::registry::RegistryStateInner {
        content: std::sync::Arc::new(crate::abcdn::catalyst_source::CatalystEntitySource::new(
            catalyst, worlds,
        )),
        manifests: dcl_contents::manifest_store::AbManifestStore::new(dir),
        profile_images_url: "https://profile-images.example".to_string(),
        world_policy: std::sync::Arc::new(dcl_contents::registry::OpenWorldPolicy),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registry_proxy_profiles_rewrites_avatars() {
    use axum::extract::State;
    let dir = lane_temp_dir("regprof");
    let addr = "0x24e5f44999c151f08609f8e27b2238c773c4d020";
    let upstream = serde_json::json!([{
        "id": "bafkprofproxy",
        "type": "profile",
        "timestamp": 1782484179697i64,
        "pointers": [addr],
        "content": [],
        "metadata": {"avatars": [{
            "name": "Proxy",
            "avatar": {"snapshots": {"face256": "https://leaky.example/f", "body": "https://leaky.example/b"}}
        }]},
    }]);
    let (chost, cseen) = crate::live::stub::serve(vec![(
        "/entities/active".to_string(),
        200,
        upstream.to_string().into_bytes(),
    )]);
    let state = mk_registry_state(&dir, &format!("http://{chost}"), None);

    let axum::Json(out) = dcl_contents::handlers::profiles::post_profiles(
        State(state),
        axum::Json(dcl_contents::types::IdsBody {
            ids: vec![addr.to_string()],
        }),
    )
    .await
    .unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["timestamp"], serde_json::json!(1782484179697i64));
    assert_eq!(
        out[0]["avatars"][0]["avatar"]["snapshots"],
        serde_json::json!({
            "face256": "https://profile-images.example/entities/bafkprofproxy/face.png",
            "body": "https://profile-images.example/entities/bafkprofproxy/body.png"
        })
    );
    assert!(cseen
        .lock()
        .unwrap()
        .contains(&"POST /entities/active".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registry_proxy_status_resolves_ids_and_reads_manifests() {
    use axum::extract::{Path, Query, State};
    let dir = lane_temp_dir("regstatus");
    let id = "bafkstatusproxy";
    for platform in ["windows", "mac", "linux"] {
        let pdir = dir.join(id);
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(
            pdir.join(format!("{platform}.manifest.json")),
            r#"{"version":"v41","exitCode":0,"date":"2024-03-15T12:34:56.789Z"}"#,
        )
        .unwrap();
    }
    let upstream = serde_json::json!([{
        "id": id,
        "type": "scene",
        "timestamp": 5i64,
        "pointers": ["0,0"],
        "content": [],
        "metadata": {},
    }]);
    let (chost, _cseen) = crate::live::stub::serve(vec![(
        "/entities/active".to_string(),
        200,
        upstream.to_string().into_bytes(),
    )]);
    let state = mk_registry_state(&dir, &format!("http://{chost}"), None);

    let axum::Json(got) = dcl_contents::handlers::status::get_entity_status(
        State(state),
        Path(id.to_string()),
        Query(dcl_contents::types::WorldNameQuery { world_name: None }),
    )
    .await
    .unwrap();
    assert_eq!(got.entity_id, id);
    assert!(got.complete);
    assert!(matches!(
        got.asset_bundles.windows,
        dcl_contents::types::BuildStatus::Complete
    ));
    assert!(got.lods.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registry_proxy_world_manifest_from_worlds_server() {
    use axum::extract::{Path, State};
    use axum::response::IntoResponse;
    let _env = lock_private_base_url_env().await;
    let dir = lane_temp_dir("regworld");
    let cid = "bafkworldproxy";
    std::env::set_var("ABGEN_ALLOW_PRIVATE_BASE_URL", "1");

    let entity = serde_json::json!({
        "id": cid,
        "type": "scene",
        "timestamp": 777i64,
        "pointers": ["myworld.dcl.eth"],
        "content": [],
        "metadata": {
            "worldConfiguration": {"name": "myworld.dcl.eth"},
            "scene": {"base": "1,1", "parcels": ["1,1", "1,2"]},
        },
    });
    let (chost, _cseen) = crate::live::stub::serve(vec![(
        format!("/contents/{cid}"),
        200,
        entity.to_string().into_bytes(),
    )]);
    let about = serde_json::json!({
        "configurations": {
            "scenesUrn": [format!(
                "urn:decentraland:entity:{cid}?=&baseUrl=http://{chost}/contents/"
            )]
        }
    });
    let (whost, _wseen) = crate::live::stub::serve(vec![(
        "/world/myworld.dcl.eth/about".to_string(),
        200,
        about.to_string().into_bytes(),
    )]);
    let state = mk_registry_state(&dir, "http://127.0.0.1:9", Some(format!("http://{whost}")));

    let axum::Json(m) = dcl_contents::handlers::worlds::get_world_manifest(
        State(state.clone()),
        Path("myworld.dcl.eth".to_string()),
    )
    .await
    .unwrap();
    assert_eq!(m.occupied, vec!["1,1".to_string(), "1,2".to_string()]);
    assert_eq!((m.spawn_coordinate.x, m.spawn_coordinate.y), (1, 1));
    assert_eq!(m.total, 2);

    let missing = dcl_contents::handlers::worlds::get_world_manifest(
        State(state),
        Path("nosuch.dcl.eth".to_string()),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(missing.into_response().status().as_u16(), 404);
    std::env::remove_var("ABGEN_ALLOW_PRIVATE_BASE_URL");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registry_proxy_maps_upstream_failures() {
    use axum::extract::{Path, Query, State};
    use axum::response::IntoResponse;
    let dir = lane_temp_dir("regdown");

    let down = mk_registry_state(&dir, "http://127.0.0.1:9", None);
    let err = dcl_contents::handlers::profiles::post_profiles(
        State(down),
        axum::Json(dcl_contents::types::IdsBody {
            ids: vec!["0xdead".to_string()],
        }),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(err.into_response().status().as_u16(), 502);

    let (ehost, _eseen) =
        crate::live::stub::serve(vec![("/entities/active".to_string(), 200, b"[]".to_vec())]);
    let empty = mk_registry_state(&dir, &format!("http://{ehost}"), None);
    let miss = dcl_contents::handlers::status::get_entity_status(
        State(empty),
        Path("bafkmissingentity".to_string()),
        Query(dcl_contents::types::WorldNameQuery { world_name: None }),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(miss.into_response().status().as_u16(), 404);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_reports_registry_mode() {
    use axum::extract::State;
    let dir = lane_temp_dir("healthreg");
    let state = mk_lane_state(&dir, None);
    let resp = super::health(State(state)).await;
    let body: serde_json::Value = serde_json::from_slice(&body_bytes(resp).await).unwrap();
    assert_eq!(body["registry"], "catalyst-proxy");
    let _ = std::fs::remove_dir_all(&dir);
}
