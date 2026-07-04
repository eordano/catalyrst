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

use self::config::Config;

pub use self::state::{AppState, AppStateInner};

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let out_root = PathBuf::from(&cfg.abgen_out_root);

    let content =
        crate::catalyst::CatalystClient::from_args(&cfg.content_url, cfg.content_disk.as_deref());

    let index_root = out_root.clone();
    let bundle_index = tokio::task::spawn_blocking(move || build_bundle_index(&index_root))
        .await
        .unwrap_or_default();
    tracing::info!(
        entries = bundle_index.len(),
        out_root = %out_root.display(),
        "ab-cdn no-deps bundle index built"
    );

    if std::env::var("ABGEN_LIVE_INPROCESS").is_ok() {
        tracing::warn!(
            "ABGEN_LIVE_INPROCESS is set but no longer used — in-process conversion \
             is always on; the variable is ignored"
        );
    }
    let pcfg = crate::live::ProxyConfig {
        catalyst_url: cfg.content_url.clone(),
        local_root: cfg.content_disk.clone(),
        cache_dir: cfg.live_cache_dir.clone(),
        version: cfg.live_version.clone(),
        template_root: cfg.abgen_root.clone(),
        ..Default::default()
    };

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

    let content_db = match &cfg.content_database_url {
        Some(url) => match sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect(url)
            .await
        {
            Ok(pool) => {
                tracing::info!("ab-cdn folded index: content DB connected");
                Some(catalyrst_registry::ports::content::ContentComponent::new(
                    pool,
                ))
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
        templates_missing,
        cfg.live_version.clone(),
        ab_date,
        content_db,
    )))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/ping", get(handlers::ping))
        .route("/health", get(handlers::health))
        .route("/entities/versions", post(handlers::post_entities_versions))
        .route("/entities/active", post(handlers::post_entities_active))
        .fallback(handlers::dispatch)
}

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
            tracing::info!(
                "folded registry routes active (profiles, worlds, status, queues, admin)"
            );
            Some(s)
        }
        Err(e) => {
            tracing::warn!(error = %e, "folded registry routes disabled: state build failed");
            None
        }
    }
}

pub fn build_app(state: AppState, registry: Option<catalyrst_registry::AppState>) -> Router {
    let mut app = api_router().with_state(state);
    if let Some(reg) = registry {
        app = app.merge(catalyrst_registry::extra_router().with_state(reg));
    }
    app
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
    let platform = ["_windows", "_mac", "_linux", "_webgl"]
        .into_iter()
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
        assert_eq!(nodeps_key("QmTiVy_mac"), None);
        assert_eq!(nodeps_key("mac.manifest.json"), None);
        assert_eq!(
            nodeps_key("QmTiVy_nothex_nothexnothexnothexnothex_mac"),
            None
        );
    }
}
