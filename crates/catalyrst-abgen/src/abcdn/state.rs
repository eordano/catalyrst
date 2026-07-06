use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;

use super::lodjit::LodJit;
use crate::catalyst::CatalystClient;
use catalyrst_registry::ports::content::ContentComponent;
use moka::future::Cache;

pub type ResolveCache = Cache<String, Option<(PathBuf, u64)>>;

pub struct IndexBuild {
    pub eager: bool,
    pub platforms: Vec<String>,
    pub sem: Arc<tokio::sync::Semaphore>,
    pub deadline: Duration,
    pub max_queue: usize,
    pub pending: Arc<AtomicUsize>,
}

impl IndexBuild {
    pub fn disabled() -> Self {
        Self {
            eager: false,
            platforms: Vec::new(),
            sem: Arc::new(tokio::sync::Semaphore::new(1)),
            deadline: Duration::from_secs(0),
            max_queue: 0,
            pending: Arc::new(AtomicUsize::new(0)),
        }
    }
}

pub struct AppStateInner {
    pub out_root: PathBuf,
    pub resolve_cache: ResolveCache,

    pub content: CatalystClient,

    pub bundle_index: HashMap<String, PathBuf>,

    pub live_proxy: Option<Arc<crate::live::Proxy>>,

    pub manifest_content_server_url: String,

    pub live_template_ok: bool,

    pub templates_missing: Vec<String>,

    pub ab_version: String,

    pub ab_date: String,

    pub content_db: Option<ContentComponent>,

    pub catalyst_url: String,

    pub out_root_writable: bool,

    pub lod_jit: LodJit,

    pub index_build: IndexBuild,

    pub worlds_content_url: Option<String>,

    pub shader_jit: bool,

    pub hash_neg_cache: Cache<String, ()>,

    pub jit_fail_cache: Cache<String, ()>,

    pub jit_inflight: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

pub type AppState = Arc<AppStateInner>;

fn env_ttl(name: &str, default_secs: u64) -> Duration {
    let secs = std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(default_secs);
    Duration::from_secs(secs.max(1))
}

impl AppStateInner {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        out_root: PathBuf,
        content: CatalystClient,
        bundle_index: HashMap<String, PathBuf>,
        live_proxy: Option<Arc<crate::live::Proxy>>,
        manifest_content_server_url: String,
        live_template_ok: bool,
        templates_missing: Vec<String>,
        ab_version: String,
        ab_date: String,
        content_db: Option<ContentComponent>,
        catalyst_url: String,
        out_root_writable: bool,
        lod_jit: LodJit,
        index_build: IndexBuild,
    ) -> Self {
        Self {
            out_root,
            resolve_cache: Cache::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            content,
            bundle_index,
            live_proxy,
            manifest_content_server_url,
            live_template_ok,
            templates_missing,
            ab_version,
            ab_date,
            content_db,
            catalyst_url,
            out_root_writable,
            lod_jit,
            index_build,
            worlds_content_url: None,
            shader_jit: crate::clihelp::env_bool("ABGEN_SHADER_JIT", true),
            hash_neg_cache: Cache::builder()
                .max_capacity(100_000)
                .time_to_live(env_ttl("ABGEN_HASH_RESOLVE_FAIL_TTL_S", 3600))
                .build(),
            jit_fail_cache: Cache::builder()
                .max_capacity(100_000)
                .time_to_live(env_ttl("ABGEN_JIT_FAIL_TTL_S", 60))
                .build(),
            jit_inflight: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn with_worlds_content_url(mut self, url: Option<String>) -> Self {
        self.worlds_content_url = url;
        self
    }
}
