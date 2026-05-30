pub mod cache;
pub mod config;
pub mod handlers;
pub mod origin;
pub mod queue;
pub mod render;
pub mod resolver;

use std::sync::Arc;

use crate::cache::ImageCache;
use crate::config::{BackendKind, Config};
use crate::origin::Origin;
use crate::queue::RenderQueue;
use crate::render::GodotRenderer;
use crate::resolver::ProfileResolver;

pub struct AppStateInner {
    pub cache: ImageCache,
    /// Local render pipeline. `Some` when the backend is `render`.
    pub render_queue: Option<RenderQueue>,
    /// `Some` when the proxy origin is available — either the primary backend
    /// (`proxy`) or the explicit `render` fallback. The handler only consults
    /// it for `render` misses when `render_fallback_proxy` is set.
    pub origin: Option<Origin>,
    /// Whether a render failure may fall through to the proxy origin.
    pub render_fallback_proxy: bool,
    pub backend_label: &'static str,
}

pub type AppState = Arc<AppStateInner>;

pub fn build_state(cfg: &Config) -> AppState {
    let cache = ImageCache::new(cfg.cache_dir.clone(), cfg.cache_ttl_seconds);

    let render_queue = match (cfg.backend_kind, &cfg.render, &cfg.content_base) {
        (BackendKind::Render, Some(rc), Some(content_base)) => {
            let resolver = ProfileResolver::new(content_base.clone());
            let renderer = GodotRenderer::new(rc.clone());
            // Cache instance for the queue is a cheap re-open of the same root.
            let queue_cache = ImageCache::new(cfg.cache_dir.clone(), cfg.cache_ttl_seconds);
            Some(RenderQueue::new(
                queue_cache,
                resolver,
                renderer,
                rc.max_concurrent,
                rc.workdir_root.clone(),
            ))
        }
        _ => None,
    };

    // Origin is built whenever an origin URL exists AND it is reachable as a
    // backend in this mode: primary proxy, or the render fallback.
    let origin = match cfg.backend_kind {
        BackendKind::Proxy => cfg.origin_url.clone().map(Origin::new),
        BackendKind::Render if cfg.render_fallback_proxy => {
            cfg.origin_url.clone().map(Origin::new)
        }
        _ => None,
    };

    Arc::new(AppStateInner {
        cache,
        render_queue,
        origin,
        render_fallback_proxy: cfg.render_fallback_proxy,
        backend_label: cfg.backend_kind.label(),
    })
}

pub fn api_router() -> axum::Router<AppState> {
    use axum::routing::get;
    axum::Router::new()
        // Upstream contract (S3/CDN key shape):
        //   GET /entities/{entity}/face.png
        //   GET /entities/{entity}/body.png
        .route("/entities/{entity}/face.png", get(handlers::images::face))
        .route("/entities/{entity}/body.png", get(handlers::images::body))
}
