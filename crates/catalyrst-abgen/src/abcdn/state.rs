use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::catalyst::CatalystClient;
use catalyrst_registry::ports::content::ContentComponent;
use moka::future::Cache;

pub type ResolveCache = Cache<String, Option<(PathBuf, u64)>>;

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
}

pub type AppState = Arc<AppStateInner>;

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
        }
    }
}
