use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, Semaphore};

use crate::cache::{ImageCache, ImageKind};
use crate::render::{GodotRenderer, RenderError};
use crate::resolver::{ProfileResolver, ResolveResult};

#[derive(Clone, Debug)]
pub enum RenderOutcome {
    Rendered,

    NotFound,

    Failed(String),
}

struct Inner {
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

    pub async fn render_once(&self, entity: &str) -> RenderOutcome {
        if self.both_cached(entity).await {
            return RenderOutcome::Rendered;
        }

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
            return match rx.recv().await {
                Ok(outcome) => outcome,
                Err(_) => Box::pin(self.render_once(entity)).await,
            };
        }

        let mut guard = InflightGuard {
            inner: Arc::clone(&self.inner),
            entity: entity.to_string(),
            tx,
            outcome: None,
        };

        let outcome = self.do_render(entity).await;
        guard.outcome = Some(outcome.clone());
        outcome
    }

    async fn both_cached(&self, entity: &str) -> bool {
        self.inner
            .cache
            .get(entity, ImageKind::Body)
            .await
            .is_some()
            && self
                .inner
                .cache
                .get(entity, ImageKind::Face)
                .await
                .is_some()
    }

    async fn do_render(&self, entity: &str) -> RenderOutcome {
        let _permit = match self.inner.limiter.acquire().await {
            Ok(p) => p,
            Err(_) => return RenderOutcome::Failed("render semaphore closed".into()),
        };

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

        let workdir =
            self.inner
                .workdir_root
                .join(format!("render-{}-{}", entity, std::process::id()));

        let result = self
            .inner
            .renderer
            .render(
                entity,
                &avatar,
                self.inner.resolver.content_base(),
                &workdir,
            )
            .await;

        let outcome = match result {
            Ok(out) => {
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

struct InflightGuard {
    inner: Arc<Inner>,
    entity: String,
    tx: broadcast::Sender<RenderOutcome>,
    outcome: Option<RenderOutcome>,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        {
            let mut map = self
                .inner
                .inflight
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            map.remove(&self.entity);
        }
        let outcome = self
            .outcome
            .take()
            .unwrap_or_else(|| RenderOutcome::Failed("render task aborted".into()));
        let _ = self.tx.send(outcome);
    }
}
