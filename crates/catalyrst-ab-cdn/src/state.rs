use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use abgen::catalyst::CatalystClient;
use catalyrst_registry::ports::content::ContentComponent;
use moka::future::Cache;

pub type ResolveCache = Cache<String, Option<(PathBuf, u64)>>;

pub struct AppStateInner {
    pub out_root: PathBuf,
    pub resolve_cache: ResolveCache,
    /// Disk-or-remote content source (entity JSON + content files). Powers native
    /// content passthrough (scene.json / main.crdt / bin/*) — replaces the nginx
    /// content-passthrough stopgap.
    pub content: CatalystClient,
    /// No-deps bundle index: lowercase `"<contentHash>_<platform>[.br]"` -> the
    /// on-disk full-form bundle path `"<contentHash>_<depsHash>_<platform>[.br]"`.
    /// Lets the resolver serve the legacy v0-abgen no-deps URL straight from the
    /// corpus with the deps hash stripped — replaces the 195k hardlink aliases.
    pub bundle_index: HashMap<String, PathBuf>,
    /// In-process live converter — always constructed (no opt-in gate). On a corpus
    /// miss the bundle is built here via the embedded abgen converter instead of
    /// HTTP-proxying to a separate abgen-serve process. The build writes its output
    /// into `out_root` in the corpus layout, then the normal corpus path re-serves
    /// it — so a JIT-converted entity is indistinguishable from a batch-converted
    /// one (the transparency invariant). `Option` retained only so dispatch can be
    /// written defensively; in practice it is always `Some`.
    pub live_proxy: Option<Arc<abgen::live::Proxy>>,
    /// `contentServerUrl` stamped into JIT-written per-entity manifests; matched
    /// to the offline corpus build so the manifests are byte-identical.
    pub manifest_content_server_url: String,
    /// Whether the in-process converter's build template was found at startup.
    /// False here with a live proxy present means every corpus miss will 500 —
    /// surfaced in /health so a missing/misconfigured ABGEN_ROOT is caught at boot
    /// instead of as silent per-request failures.
    pub live_template_ok: bool,
    /// AB version the JIT converter stamps; reported by the index for buildable
    /// (not-yet-on-disk) entities so the advertised version is stable across the
    /// JIT build. Mirrors the converter's `ABGEN_VERSION`.
    pub ab_version: String,
    /// Content-DB component for the folded index (pointer→entity with real
    /// timestamp/deployer/content/metadata). `None` when no content DB is
    /// configured — the index then falls back to the content client.
    pub content_db: Option<ContentComponent>,
}

pub type AppState = Arc<AppStateInner>;

impl AppStateInner {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        out_root: PathBuf,
        content: CatalystClient,
        bundle_index: HashMap<String, PathBuf>,
        live_proxy: Option<Arc<abgen::live::Proxy>>,
        manifest_content_server_url: String,
        live_template_ok: bool,
        ab_version: String,
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
            ab_version,
            content_db,
        }
    }
}
