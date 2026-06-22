//! Unified content / asset-bundle server. One server behind a single resolver
//! doing all three jobs: serve a pre-built bundle from the corpus (`ABGEN_OUT_ROOT`),
//! on a miss convert the entity in-process and write it into the corpus then
//! re-serve it, and pass non-AB content (`scene.json` / `main.crdt` / `bin/*`)
//! straight from the content store. Corpus-hit and JIT-miss are byte-and-header
//! identical (a client cannot tell which built an asset). No standalone abgen-serve,
//! no nginx passthrough, no hardlink/`_mac` aliases. See `docs/unified-content-ab-server.md`.

pub mod config;
pub mod handlers;
pub mod index;
pub mod resolver;
pub mod serve;
pub mod state;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use axum::routing::{get, post};
use axum::Router;

use crate::config::Config;

pub use state::{AppState, AppStateInner};

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let out_root = PathBuf::from(&cfg.abgen_out_root);

    // Disk-or-remote content source. `content_disk` (when set) is the on-disk
    // content store; otherwise fetches go over HTTP to `content_url`.
    let content =
        abgen::catalyst::CatalystClient::from_args(&cfg.content_url, cfg.content_disk.as_deref());

    // Build the no-deps bundle index once at startup (single scandir of the
    // corpus root). Replaces the hardlink-alias stopgap: the legacy v0-abgen
    // no-deps URL resolves to the on-disk full-form bundle with deps stripped.
    let index_root = out_root.clone();
    let bundle_index = tokio::task::spawn_blocking(move || build_bundle_index(&index_root))
        .await
        .unwrap_or_default();
    tracing::info!(
        entries = bundle_index.len(),
        out_root = %out_root.display(),
        "ab-cdn no-deps bundle index built"
    );

    // Live conversion is now ALWAYS folded in-process: on a corpus miss the bundle
    // is built here via the embedded abgen converter and written into the corpus,
    // so there is no separate abgen-serve process and no opt-in gate. (The old
    // ABGEN_LIVE_INPROCESS toggle is gone; warn if an operator still sets it.)
    if std::env::var("ABGEN_LIVE_INPROCESS").is_ok() {
        tracing::warn!(
            "ABGEN_LIVE_INPROCESS is set but no longer used — in-process conversion \
             is always on; the variable is ignored"
        );
    }
    let pcfg = abgen::live::ProxyConfig {
        catalyst_url: cfg.content_url.clone(),
        local_root: cfg.content_disk.clone(),
        cache_dir: cfg.live_cache_dir.clone(),
        version: cfg.live_version.clone(),
        template_root: cfg.abgen_root.clone(),
        ..Default::default()
    };
    // Proxy::new exports ABGEN_ROOT, so validate the template AFTER it.
    let live_proxy = abgen::live::Proxy::new(pcfg);
    let live_template_ok = abgen::builder::template_available();
    if live_template_ok {
        tracing::info!(
            turbojpeg = abgen::live::Proxy::turbojpeg_available(),
            template_dir = %abgen::builder::template_dir().display(),
            cache = %cfg.live_cache_dir,
            version = %cfg.live_version,
            "ab-cdn in-process conversion active"
        );
    } else {
        // Fail loud at boot: without the template, every corpus miss 500s (this is
        // the wearable-500 root cause). Set ABGEN_ROOT to the dir holding
        // template/all-types.windows.bundle.
        tracing::error!(
            template_dir = %abgen::builder::template_dir().display(),
            abgen_root = ?cfg.abgen_root,
            "ab-cdn in-process converter active but build template MISSING — \
             every corpus miss will 500; set ABGEN_ROOT to the template root"
        );
    }
    let live_proxy = Some(live_proxy);

    // Content-DB component for the folded index (entities/active + /versions):
    // real timestamp/deployer/content/metadata via the same query the registry
    // uses. Optional — if unconfigured or the pool fails, the index falls back to
    // the content client (no timestamp/deployer) and AB serving is unaffected.
    let content_db = match &cfg.content_database_url {
        Some(url) => match sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect(url)
            .await
        {
            Ok(pool) => {
                tracing::info!("ab-cdn folded index: content DB connected");
                Some(catalyrst_registry::ports::content::ContentComponent::new(pool))
            }
            Err(e) => {
                tracing::warn!(error = %e, "ab-cdn folded index: content DB unavailable — entities/active+versions fall back to the content client (no timestamp/deployer)");
                None
            }
        },
        None => None,
    };

    Ok(Arc::new(AppStateInner::new(
        out_root,
        content,
        bundle_index,
        live_proxy,
        cfg.manifest_content_server_url.clone(),
        live_template_ok,
        cfg.live_version.clone(),
        content_db,
    )))
}

/// The unified server's own routes (its `AppState`): health, the folded
/// AB-availability index (servability-derived + JIT-aware, from this server's
/// out_root), and the AB-serve dispatch as the FALLBACK — so any registry routes
/// merged on top (see `extra_router` / `build_app`) take precedence and everything
/// else (manifests, LOD, bundles, native content) is served.
pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/ping", get(handlers::ping))
        .route("/health", get(handlers::health))
        .route("/entities/versions", post(handlers::post_entities_versions))
        .route("/entities/active", post(handlers::post_entities_active))
        .fallback(handlers::dispatch)
}

/// Optionally build the standalone ab-registry's `AppState` so the unified server
/// can mount the rest of its routes (`extra_router`) and fully subsume it. Reads
/// the same env (content DB, registry DB, profile base, ABGEN_OUT_ROOT — pointed
/// at THIS server's corpus). Returns `None` (logged) when the registry's data
/// sources aren't configured — the unified server still serves ABs + the index.
pub async fn build_registry_state() -> Option<catalyrst_registry::AppState> {
    let cfg = match catalyrst_registry::config::Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "folded registry routes disabled: config unavailable");
            return None;
        }
    };
    match catalyrst_registry::build_state(&cfg).await {
        Ok(s) => {
            tracing::info!("folded registry routes active (profiles, worlds, status, queues, admin)");
            Some(s)
        }
        Err(e) => {
            tracing::warn!(error = %e, "folded registry routes disabled: state build failed");
            None
        }
    }
}

/// Assemble the full unified-server app: the unified routes + AB-serve fallback,
/// with the rest of the registry merged on top when available. The registry's
/// `extra_router` carries its own (state-erased) `AppState`; the unified routes +
/// fallback carry this server's. Specific registry routes win; the fallback serves
/// the rest.
pub fn build_app(state: AppState, registry: Option<catalyrst_registry::AppState>) -> Router {
    let mut app = api_router().with_state(state);
    if let Some(reg) = registry {
        app = app.merge(catalyrst_registry::extra_router().with_state(reg));
    }
    app
}

/// Scan the flat corpus root for full-form bundles `<hash>_<32hex deps>_<platform>[.br]`
/// and index each by its no-deps key `<hash>_<platform>[.br]` (lowercased).
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
            // First writer wins; multiple deps for one (hash,platform) collapse to
            // one bundle, matching the prior hardlink-alias dedup semantics.
            idx.entry(key).or_insert_with(|| ent.path());
        }
    }
    idx
}

/// `"<hash>_<32hex deps>_<platform>[.br]"` -> `Some("<hash>_<platform>[.br]")` (lowercased).
/// Returns None for names that aren't full-form bundles (manifests, no-deps names, etc.).
fn nodeps_key(name: &str) -> Option<String> {
    let (base, br) = match name.strip_suffix(".br") {
        Some(b) => (b, ".br"),
        None => (name, ""),
    };
    let platform = ["_windows", "_mac", "_linux", "_webgl"]
        .into_iter()
        .find(|p| base.ends_with(p))?;
    let stem = base.strip_suffix(platform)?; // "<hash>_<deps>"
    let (hash, deps) = stem.rsplit_once('_')?; // "<hash>", "<deps>"
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
        // We can't runtime-test the merged app here, but Router construction panics
        // on duplicate routes. The unified router (fallback + index) and the folded
        // registry extra_router must each build, and their route sets are disjoint
        // (registry has no /ping,/health,/entities/active,/entities/versions, and
        // the unified server uses a fallback, not a catch-all route, so specific
        // registry routes can be merged on top).
        let _ = super::api_router();
        let _ = catalyrst_registry::extra_router();
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
        assert_eq!(nodeps_key("QmTiVy_mac"), None); // already no-deps (deps != 32 hex)
        assert_eq!(nodeps_key("mac.manifest.json"), None);
        assert_eq!(nodeps_key("QmTiVy_nothex_nothexnothexnothexnothex_mac"), None);
    }
}
