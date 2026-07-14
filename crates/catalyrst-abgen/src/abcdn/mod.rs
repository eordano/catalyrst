pub mod catalyst_source;
pub mod config;
pub mod handlers;
pub mod index;
pub mod jitcache;
pub mod lodjit;
pub mod metrics;
pub mod range;
pub mod resolver;
pub mod serve;
pub mod state;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use axum::routing::{get, post};
use axum::Router;

use self::config::Config;

pub use self::state::{AppState, AppStateInner};

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let out_root = PathBuf::from(&cfg.abgen_out_root);

    tracing::info!(
        catalyst = %cfg.content_url,
        out_root = %cfg.abgen_out_root,
        cache = %cfg.live_cache_dir,
        version = %cfg.live_version,
        content_disk = ?cfg.content_disk,
        abgen_root = ?cfg.abgen_root,
        git = option_env!("ABGEN_GIT_COMMIT").unwrap_or("unknown"),
        "abgen server config"
    );

    if let Err(e) = std::fs::create_dir_all(&out_root) {
        tracing::error!(
            error = %e,
            out_root = %out_root.display(),
            "ABGEN_OUT_ROOT cannot be created — the corpus will serve empty and JIT \
             write-back will fail; fix the path or permissions"
        );
    }
    let out_root_writable = probe_writable(&out_root);
    if !out_root_writable {
        tracing::error!(
            out_root = %out_root.display(),
            "ABGEN_OUT_ROOT is not writable — JIT conversions will build but fail to persist"
        );
    }

    let roots_distinct = {
        let jit_probe = PathBuf::from(&cfg.live_cache_dir);
        let _ = std::fs::create_dir_all(&jit_probe);
        roots_are_distinct(&out_root, &jit_probe)
    };

    let content =
        crate::catalyst::CatalystClient::from_args(&cfg.content_url, cfg.content_disk.as_deref());

    probe_catalyst(&cfg.content_url).await;

    let index_root = out_root.clone();
    let bundle_index = tokio::task::spawn_blocking(move || build_bundle_index(&index_root))
        .await
        .unwrap_or_default();
    tracing::info!(
        entries = bundle_index.len(),
        out_root = %out_root.display(),
        "ab-cdn no-deps bundle index built"
    );
    metrics::gauge_bundle_index(bundle_index.len());

    let use_space = std::env::var("ABGEN_S3_BUCKET").is_ok_and(|v| !v.is_empty())
        || crate::clihelp::env_bool("ABGEN_USE_SPACE", false);
    let fallback_version = std::env::var("ABGEN_FALLBACK_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "v41".to_string());
    let pcfg = crate::live::ProxyConfig {
        catalyst_url: cfg.content_url.clone(),
        local_root: cfg.content_disk.clone(),
        cache_dir: cfg.live_cache_dir.clone(),
        version: cfg.live_version.clone(),
        template_root: cfg.abgen_root.clone(),
        use_space,
        fallback_version,
        ..Default::default()
    };
    if use_space {
        let space_read_only = crate::clihelp::env_bool("ABGEN_S3_READ_ONLY", false);
        tracing::info!(
            read_only = space_read_only,
            "ab-cdn S3 space cache ENABLED (read-through + write-back)"
        );
    }

    let live_proxy = crate::live::Proxy::new(pcfg);
    let ab_date = live_proxy.date().to_string();
    let live_template_ok = crate::builder::template_available();
    let templates_missing = crate::builder::templates_missing();
    if live_template_ok {
        tracing::info!(
            turbojpeg = crate::live::Proxy::turbojpeg_available(),
            template_dir = %crate::builder::template_dir().display(),
            cache = %cfg.live_cache_dir,
            version = %cfg.live_version,
            build_date = %ab_date,
            "ab-cdn in-process conversion active"
        );
    } else {
        tracing::error!(
            template_dir = %crate::builder::template_dir().display(),
            abgen_root = ?cfg.abgen_root,
            "ab-cdn in-process converter active but build template MISSING — \
             every corpus miss will 500; set ABGEN_ROOT to the template root"
        );
    }
    if !templates_missing.is_empty() {
        tracing::error!(
            missing = ?templates_missing,
            template_dir = %crate::builder::template_dir().display(),
            "ab-cdn required build templates MISSING — bundles built without \
             them lose animation/skinned emission; /health reports degraded"
        );
    }
    let live_proxy = Some(live_proxy);

    #[cfg(feature = "content-db")]
    let content_db = match &cfg.content_database_url {
        Some(url) => match sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect(url)
            .await
        {
            Ok(pool) => {
                tracing::info!("ab-cdn folded index: content DB connected");
                Some(dcl_contents::content::ContentComponent::new(pool))
            }
            Err(e) => {
                tracing::warn!(error = %e, "ab-cdn folded index: content DB unavailable — entities/active+versions fall back to the content client (no timestamp/deployer)");
                None
            }
        },
        None => None,
    };
    #[cfg(feature = "content-db")]
    let db_source: Option<Arc<dyn dcl_contents::registry::EntitySource>> = content_db
        .clone()
        .map(|c| Arc::new(c) as Arc<dyn dcl_contents::registry::EntitySource>);
    #[cfg(not(feature = "content-db"))]
    let db_source: Option<Arc<dyn dcl_contents::registry::EntitySource>> = None;
    let registry_source: Arc<dyn dcl_contents::registry::EntitySource> = match db_source {
        Some(source) => {
            tracing::info!("registry routes: content DB");
            source
        }
        None => {
            tracing::info!(catalyst = %cfg.content_url, "registry routes: catalyst proxy");
            Arc::new(catalyst_source::CatalystEntitySource::new(
                &cfg.content_url,
                crate::worlds::content_fallback_from_env(),
            ))
        }
    };
    let contents_registry = Arc::new(dcl_contents::registry::RegistryStateInner {
        content: registry_source,
        manifests: {
            let store = dcl_contents::manifest_store::AbManifestStore::new(out_root.clone());
            let jit_root = PathBuf::from(&cfg.live_cache_dir);
            if roots_distinct {
                store.with_fallback_root(jit_root)
            } else {
                store
            }
        },
        profile_images_url: std::env::var("PROFILE_CDN_BASE_URL")
            .or_else(|_| std::env::var("PROFILE_IMAGES_URL"))
            .unwrap_or_else(|_| "https://profile-images.decentraland.org".to_string())
            .trim_end_matches('/')
            .to_string(),
        world_policy: Arc::new(dcl_contents::registry::OpenWorldPolicy),
    });

    let lod_jit = lodjit::LodJit::from_env(&cfg.live_cache_dir);
    if lod_jit.enabled {
        if std::env::var_os(crate::lodgen::simplify::SUBPROC_TIMEOUT_ENV).is_none() {
            std::env::set_var(
                crate::lodgen::simplify::SUBPROC_TIMEOUT_ENV,
                lod_jit.timeout.as_secs().to_string(),
            );
        }
        tracing::info!(
            gltfpack = %lod_jit.gltfpack.as_deref().unwrap_or(Path::new("?")).display(),
            manifest_builder = lod_jit.manifest_builder.is_some(),
            cache = %lod_jit.cache_dir.display(),
            workdir = %lod_jit.workdir.display(),
            timeout_s = lod_jit.timeout.as_secs(),
            "ab-cdn LOD JIT lane ENABLED (ABGEN_LOD_JIT)"
        );
    }

    let index_eager = crate::clihelp::env_bool("ABGEN_INDEX_EAGER_BUILD", true);
    let index_platforms: Vec<String> = std::env::var("ABGEN_INDEX_BUILD_PLATFORMS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "windows,mac".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let index_concurrency = std::env::var("ABGEN_INDEX_BUILD_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(2)
        });
    let index_deadline_ms = std::env::var("ABGEN_INDEX_BUILD_DEADLINE_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(20_000);
    let index_max_queue = std::env::var("ABGEN_INDEX_BUILD_MAX_QUEUE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    if index_eager && live_proxy.is_some() {
        tracing::info!(
            platforms = ?index_platforms,
            concurrency = index_concurrency,
            deadline_ms = index_deadline_ms,
            max_queue = index_max_queue,
            "index-hit eager conversion ENABLED"
        );
    }
    let index_build = crate::abcdn::state::IndexBuild {
        eager: index_eager,
        platforms: index_platforms,
        sem: std::sync::Arc::new(tokio::sync::Semaphore::new(index_concurrency)),
        deadline: std::time::Duration::from_millis(index_deadline_ms),
        max_queue: index_max_queue,
        pending: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };

    let jit_root = PathBuf::from(&cfg.live_cache_dir);
    let _ = std::fs::create_dir_all(&jit_root);
    let jit_budget = std::env::var("ABGEN_JIT_CACHE_MAX_BYTES")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(50 * 1024 * 1024 * 1024);
    let jit_budget = if roots_distinct { jit_budget } else { 0 };
    let jit_cache = jitcache::JitDiskCache::new(jit_budget);
    if let Some(p) = &live_proxy {
        p.set_jit_cache(jit_cache.clone());
    }
    if jit_cache.enabled() {
        let jit_root_seed = jit_root.clone();
        let build_roots = live_proxy.as_ref().map(|p| {
            let (content_root, bundle_dir) = p.cache_roots();
            (content_root.to_path_buf(), bundle_dir.to_path_buf())
        });
        let items = tokio::task::spawn_blocking(move || {
            let mut items = seed_jit_cache(&jit_root_seed);
            if let Some((content_root, bundle_dir)) = build_roots {
                prune_stale_bundle_dirs(&bundle_dir);
                items.extend(seed_build_caches(&content_root, &bundle_dir));
            }
            items
        })
        .await
        .unwrap_or_default();
        let seeded = items.len();
        jit_cache.seed_many(items);
        tracing::info!(
            cache = %jit_root.display(),
            budget_bytes = jit_budget,
            seeded,
            bytes = jit_cache.total_bytes(),
            "JIT disk cache: LRU + refcount enabled (entities + bundles + content)"
        );
    } else {
        tracing::info!(
            cache = %jit_root.display(),
            "JIT disk cache disabled (budget 0 or serve-cache == out_root); \
             content/ and bundles/ build caches are UNBOUNDED"
        );
    }

    let inner = AppStateInner::new(
        out_root,
        content,
        bundle_index,
        live_proxy,
        cfg.manifest_content_server_url.clone(),
        live_template_ok,
        templates_missing,
        cfg.live_version.clone(),
        ab_date,
        cfg.content_url.clone(),
        out_root_writable,
        lod_jit,
        index_build,
    )
    .with_worlds_content_url(crate::worlds::content_fallback_from_env())
    .with_jit(jit_root, jit_cache, roots_distinct)
    .with_registry_state(Some(contents_registry));
    #[cfg(feature = "content-db")]
    let inner = inner.with_content_db(content_db);
    Ok(Arc::new(inner))
}

fn seed_jit_cache(jit_root: &Path) -> Vec<(String, PathBuf, u64)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(jit_root) else {
        return out;
    };
    for ent in rd.flatten() {
        let Ok(ft) = ent.file_type() else { continue };
        let name = ent.file_name();
        let Some(name) = name.to_str() else { continue };
        let path = ent.path();
        if name.contains(jitcache::EVICTED_SUFFIX) {
            if ft.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else {
                let _ = std::fs::remove_file(&path);
            }
            continue;
        }
        if ft.is_file() && name.contains(".tmp.") {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        if matches!(
            name,
            "content" | "bundles" | "lod-work" | "lod-content" | "dcl"
        ) {
            continue;
        }
        if ft.is_dir() {
            let bytes = jitcache::dir_size(&path);
            out.push((name.to_string(), path, bytes));
        } else if ft.is_file() {
            let bytes = ent.metadata().map(|m| m.len()).unwrap_or(0);
            if bytes == 0 {
                continue;
            }
            out.push((name.to_string(), path, bytes));
        }
    }
    out
}

fn prune_stale_bundle_dirs(current: &Path) {
    let (Some(parent), Some(keep)) = (current.parent(), current.file_name()) else {
        return;
    };
    let Ok(rd) = std::fs::read_dir(parent) else {
        return;
    };
    for ent in rd.flatten() {
        if ent.file_name() == keep {
            continue;
        }
        if ent.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let _ = std::fs::remove_dir_all(ent.path());
        }
    }
}

fn seed_build_caches(content_root: &Path, bundle_dir: &Path) -> Vec<(String, PathBuf, u64)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(bundle_dir) {
        for ent in rd.flatten() {
            if !ent.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = ent.file_name();
            let Some(cid) = name.to_str() else { continue };
            let path = ent.path();
            if cid.contains(jitcache::EVICTED_SUFFIX) {
                let _ = std::fs::remove_dir_all(&path);
                continue;
            }
            let bytes = jitcache::dir_size(&path);
            out.push((format!("b:{cid}"), path, bytes));
        }
    }
    let mut stack = vec![content_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let Ok(ft) = ent.file_type() else { continue };
            if ft.is_dir() {
                stack.push(ent.path());
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            let name = ent.file_name();
            let Some(hash) = name.to_str() else { continue };
            if hash.contains(jitcache::EVICTED_SUFFIX) {
                let _ = std::fs::remove_file(ent.path());
                continue;
            }
            if hash.contains(".tmp.") {
                continue;
            }
            let bytes = ent.metadata().map(|m| m.len()).unwrap_or(0);
            out.push((format!("c:{hash}"), ent.path(), bytes));
        }
    }
    out
}

fn probe_writable(dir: &Path) -> bool {
    let p = dir.join(".abgen-write-probe");
    match std::fs::write(&p, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&p);
            true
        }
        Err(_) => false,
    }
}

fn roots_are_distinct(out_root: &Path, jit_root: &Path) -> bool {
    match (
        std::fs::canonicalize(out_root),
        std::fs::canonicalize(jit_root),
    ) {
        (Ok(a), Ok(b)) => a != b && !same_device_inode(&a, &b),
        _ => {
            tracing::warn!(
                out_root = %out_root.display(),
                jit_root = %jit_root.display(),
                "abgen: canonicalize failed for a cache root — treating jit_root and out_root as \
                 the SAME directory to disable LRU eviction (fail-safe: refuses to risk \
                 remove_dir_all-ing the immutable warm corpus)"
            );
            false
        }
    }
}

#[cfg(unix)]
fn same_device_inode(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_device_inode(_a: &Path, _b: &Path) -> bool {
    false
}

async fn probe_catalyst(content_url: &str) {
    let url = format!("{}/status", content_url.trim_end_matches('/'));
    let shown = content_url.to_string();
    let probed = tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(3)))
            .build()
            .into();
        match agent.get(&url).call() {
            Ok(r) => Ok(r.status().as_u16()),
            Err(ureq::Error::StatusCode(c)) => Ok(c),
            Err(e) => Err(e.to_string()),
        }
    })
    .await;
    match probed {
        Ok(Ok(code)) => {
            tracing::info!(status = code, url = %shown, "catalyst content server reachable")
        }
        Ok(Err(e)) => tracing::warn!(
            error = %e,
            url = %shown,
            "catalyst content server UNREACHABLE — corpus hits still serve, but every \
             JIT conversion will fail until this resolves"
        ),
        Err(_) => {}
    }
}

pub fn api_router() -> Router<AppState> {
    let metrics_token = std::env::var("ABGEN_METRICS_BEARER_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    Router::new()
        .route("/ping", get(handlers::ping))
        .route("/health", get(handlers::health))
        .route("/livez", get(handlers::livez))
        .route("/readyz", get(handlers::readyz))
        .route(
            "/metrics",
            get(move |headers: axum::http::HeaderMap| {
                let token = metrics_token.clone();
                async move { handlers::metrics(token, headers).await }
            }),
        )
        .route("/entities/versions", post(handlers::post_entities_versions))
        .route("/entities/active", post(handlers::post_entities_active))
        .fallback(handlers::dispatch)
}

pub fn build_app(state: AppState) -> Router {
    let contents_registry = state.contents_registry.clone();
    let app = api_router().with_state(state);
    let app = match contents_registry {
        Some(cs) => app.merge(dcl_contents::registry::router().with_state(cs)),
        None => app,
    };
    app.layer(axum::middleware::from_fn(metrics::track_http))
}

fn build_bundle_index(root: &Path) -> HashMap<String, PathBuf> {
    let mut idx = HashMap::new();
    let Ok(rd) = std::fs::read_dir(root) else {
        return idx;
    };
    for ent in rd.flatten() {
        if !ent.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let name = ent.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(key) = nodeps_key(name) {
            idx.entry(key).or_insert_with(|| ent.path());
        }
    }
    idx
}

fn nodeps_key(name: &str) -> Option<String> {
    let (base, br) = match name.strip_suffix(".br") {
        Some(b) => (b, ".br"),
        None => (name, ""),
    };
    let platform = resolver::PLATFORMS
        .iter()
        .map(|(suffix, _)| *suffix)
        .find(|p| base.ends_with(p))?;
    let stem = base.strip_suffix(platform)?;
    let (hash, deps) = stem.rsplit_once('_')?;
    if deps.len() != 32 || !deps.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("{}{}{}", hash.to_ascii_lowercase(), platform, br))
}

#[cfg(test)]
mod tests {
    use super::nodeps_key;

    #[test]
    fn routers_construct_without_route_conflicts() {
        let _ = super::api_router();
    }

    #[test]
    fn nodeps_key_strips_deps_and_lowercases() {
        assert_eq!(
            nodeps_key("QmTiVy_4f53cda18c2baa0c0354bb5f9a3ecbe5_mac").as_deref(),
            Some("qmtivy_mac")
        );
        assert_eq!(
            nodeps_key("QmTiVy_4f53cda18c2baa0c0354bb5f9a3ecbe5_mac.br").as_deref(),
            Some("qmtivy_mac.br")
        );
        assert_eq!(
            nodeps_key("bafkabc_0123456789abcdef0123456789abcdef_windows").as_deref(),
            Some("bafkabc_windows")
        );
        assert_eq!(
            nodeps_key("QmW_b388b86e6754ba44ef9406c0ccceb8d1_webgl").as_deref(),
            Some("qmw_webgl")
        );
    }

    #[test]
    fn nodeps_key_rejects_non_bundles() {
        assert_eq!(nodeps_key("scene.json"), None);
        assert_eq!(nodeps_key("QmTiVy_mac"), None);
        assert_eq!(nodeps_key("mac.manifest.json"), None);
        assert_eq!(
            nodeps_key("QmTiVy_nothex_nothexnothexnothexnothex_mac"),
            None
        );
    }
}
