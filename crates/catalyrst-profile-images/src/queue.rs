//! Single-flight render queue.
//!
//! A request for `/entities/{id}/face.png` and one for `.../body.png` for the
//! same `id` both need the same Godot render (one render emits *both* PNGs).
//! Many concurrent clients may ask for the same brand-new entity at once. This
//! queue guarantees:
//!
//!   * at most one in-flight render per entity id (single-flight), and
//!   * a global cap on concurrent Godot processes (a semaphore), since each
//!     render spawns a heavyweight headless client.
//!
//! Callers `await` `render_once(entity)`; the first caller for an entity does
//! the work, every other caller for the same entity parks on the same shared
//! result. On success both PNGs are already in the cache, so the handler just
//! re-reads the cache.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, Semaphore};

use crate::cache::{ImageCache, ImageKind};
use crate::render::{GodotRenderer, RenderError};
use crate::resolver::{ProfileResolver, ResolveResult};

/// Terminal outcome of a single-flight render, cloneable so it can fan out to
/// all waiters via a broadcast channel.
#[derive(Clone, Debug)]
pub enum RenderOutcome {
    /// Both PNGs were rendered and written to the cache.
    Rendered,
    /// The entity has no avatar / is not a profile / content core 404 — there
    /// is nothing to render. Maps to HTTP 404.
    NotFound,
    /// The render (or the resolve step) failed. Maps to HTTP 502.
    Failed(String),
}

struct Inner {
    /// entity id -> broadcast sender for the in-flight render's outcome.
    inflight: Mutex<HashMap<String, broadcast::Sender<RenderOutcome>>>,
    limiter: Semaphore,
    cache: ImageCache,
    resolver: ProfileResolver,
    renderer: GodotRenderer,
    workdir_root: std::path::PathBuf,
}

#[derive(Clone)]
pub struct RenderQueue {
    inner: Arc<Inner>,
}

impl RenderQueue {
    pub fn new(
        cache: ImageCache,
        resolver: ProfileResolver,
        renderer: GodotRenderer,
        max_concurrent: usize,
        workdir_root: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                inflight: Mutex::new(HashMap::new()),
                limiter: Semaphore::new(max_concurrent.max(1)),
                cache,
                resolver,
                renderer,
                workdir_root: workdir_root.into(),
            }),
        }
    }

    /// Ensure both PNGs for `entity` exist in the cache, rendering once if
    /// needed. Concurrent calls for the same entity share a single render.
    pub async fn render_once(&self, entity: &str) -> RenderOutcome {
        // Fast path: another caller already cached both PNGs.
        if self.both_cached(entity).await {
            return RenderOutcome::Rendered;
        }

        // Join or become the single-flight leader for this entity.
        let (leader, mut rx, tx) = {
            let mut map = self.inner.inflight.lock().unwrap();
            if let Some(existing) = map.get(entity) {
                (false, existing.subscribe(), existing.clone())
            } else {
                let (tx, rx) = broadcast::channel(1);
                map.insert(entity.to_string(), tx.clone());
                (true, rx, tx)
            }
        };

        if !leader {
            // Follower: wait for the leader's outcome. If the leader vanished
            // before sending (panic), fall back to a fresh attempt.
            return match rx.recv().await {
                Ok(outcome) => outcome,
                Err(_) => Box::pin(self.render_once(entity)).await,
            };
        }

        // Leader: do the work, broadcast, then deregister.
        let outcome = self.do_render(entity).await;
        {
            let mut map = self.inner.inflight.lock().unwrap();
            map.remove(entity);
        }
        // Best-effort fan-out (errors only mean nobody is waiting).
        let _ = tx.send(outcome.clone());
        outcome
    }

    async fn both_cached(&self, entity: &str) -> bool {
        self.inner.cache.get(entity, ImageKind::Body).await.is_some()
            && self.inner.cache.get(entity, ImageKind::Face).await.is_some()
    }

    /// The leader's actual work: acquire a render slot, resolve the avatar,
    /// run Godot, copy both PNGs into the cache, clean up the workdir.
    async fn do_render(&self, entity: &str) -> RenderOutcome {
        let _permit = match self.inner.limiter.acquire().await {
            Ok(p) => p,
            Err(_) => return RenderOutcome::Failed("render semaphore closed".into()),
        };

        // Re-check the cache now that we hold the slot — a render that
        // completed while we queued behind the semaphore makes ours redundant.
        if self.both_cached(entity).await {
            return RenderOutcome::Rendered;
        }

        let avatar = match self.inner.resolver.resolve(entity).await {
            ResolveResult::Avatar(v) => v,
            ResolveResult::NotFound => return RenderOutcome::NotFound,
            ResolveResult::Error(e) => {
                tracing::error!(entity = %entity, error = %e, "profile resolve failed");
                return RenderOutcome::Failed(format!("resolve: {e}"));
            }
        };

        // Per-render scratch dir; removed at the end regardless of outcome.
        let workdir = self
            .inner
            .workdir_root
            .join(format!("render-{}-{}", entity, std::process::id()));

        let result = self
            .inner
            .renderer
            .render(entity, &avatar, self.inner.resolver.content_base(), &workdir)
            .await;

        let outcome = match result {
            Ok(out) => {
                // Move both PNGs into the content-addressed cache.
                let body = tokio::fs::read(&out.body_path).await;
                let face = tokio::fs::read(&out.face_path).await;
                match (body, face) {
                    (Ok(b), Ok(f)) => {
                        let b = bytes::Bytes::from(b);
                        let f = bytes::Bytes::from(f);
                        let w1 = self.inner.cache.put(entity, ImageKind::Body, &b).await;
                        let w2 = self.inner.cache.put(entity, ImageKind::Face, &f).await;
                        match (w1, w2) {
                            (Ok(()), Ok(())) => RenderOutcome::Rendered,
                            _ => RenderOutcome::Failed("cache write failed".into()),
                        }
                    }
                    _ => RenderOutcome::Failed("rendered png unreadable".into()),
                }
            }
            Err(RenderError::OutputMissing { .. }) => {
                // Godot ran but produced nothing usable — treat as a failure so
                // the caller can fall back, but log loudly.
                tracing::warn!(entity = %entity, "godot produced no usable output");
                RenderOutcome::Failed("render produced no output".into())
            }
            Err(e) => {
                tracing::error!(entity = %entity, error = %e, "godot render failed");
                RenderOutcome::Failed(e.to_string())
            }
        };

        let _ = tokio::fs::remove_dir_all(&workdir).await;
        outcome
    }
}
