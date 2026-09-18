use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use tokio::sync::{broadcast, Semaphore};

use crate::cache::{ImageCache, ImageKind};
use crate::render::{GodotRenderer, RenderError};
use crate::resolver::{ProfileResolver, ResolveResult};

#[derive(Clone, Debug)]
pub enum RenderOutcome {
    Rendered { body: Bytes, face: Bytes },

    NotFound,

    Failed(String),
}

const NOT_FOUND_TTL: Duration = Duration::from_secs(300);
const NOT_FOUND_MAX: usize = 65536;

/// Bounded negative memo for profiles the resolver reported missing.
struct MissMemo {
    map: Mutex<HashMap<String, Instant>>,
}

impl MissMemo {
    fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }

    fn hit(&self, entity: &str) -> bool {
        self.map
            .lock()
            .ok()
            .and_then(|m| m.get(entity).copied())
            .is_some_and(|at| at.elapsed() < NOT_FOUND_TTL)
    }

    fn record(&self, entity: &str) {
        if let Ok(mut m) = self.map.lock() {
            if m.len() >= NOT_FOUND_MAX {
                m.retain(|_, at| at.elapsed() < NOT_FOUND_TTL);
            }
            if m.len() < NOT_FOUND_MAX {
                m.insert(entity.to_string(), Instant::now());
            }
        }
    }
}

struct Inner {
    inflight: Mutex<HashMap<String, broadcast::Sender<RenderOutcome>>>,
    not_found: MissMemo,
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
                not_found: MissMemo::new(),
                limiter: Semaphore::new(max_concurrent.max(1)),
                cache,
                resolver,
                renderer,
                workdir_root: workdir_root.into(),
            }),
        }
    }

    pub async fn render_once(&self, entity: &str) -> RenderOutcome {
        if self.inner.not_found.hit(entity) {
            return RenderOutcome::NotFound;
        }
        if let Some(cached) = self.cached(entity).await {
            return cached;
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

    async fn cached(&self, entity: &str) -> Option<RenderOutcome> {
        let (body, face) = tokio::join!(
            self.inner.cache.get(entity, ImageKind::Body),
            self.inner.cache.get(entity, ImageKind::Face)
        );
        Some(RenderOutcome::Rendered {
            body: body?,
            face: face?,
        })
    }

    async fn do_render(&self, entity: &str) -> RenderOutcome {
        let _permit = match self.inner.limiter.acquire().await {
            Ok(p) => p,
            Err(_) => return RenderOutcome::Failed("render semaphore closed".into()),
        };

        let avatar = match self.inner.resolver.resolve(entity).await {
            ResolveResult::Avatar(v) => v,
            ResolveResult::NotFound => {
                self.inner.not_found.record(entity);
                return RenderOutcome::NotFound;
            }
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
                let (body, face) = tokio::join!(
                    tokio::fs::read(&out.body_path),
                    tokio::fs::read(&out.face_path)
                );
                match (body, face) {
                    (Ok(b), Ok(f)) => {
                        let body = Bytes::from(b);
                        let face = Bytes::from(f);
                        let (w1, w2) = tokio::join!(
                            self.inner.cache.put(entity, ImageKind::Body, &body),
                            self.inner.cache.put(entity, ImageKind::Face, &face)
                        );
                        match (w1, w2) {
                            (Ok(()), Ok(())) => RenderOutcome::Rendered { body, face },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miss_memo_remembers_recent_misses_only() {
        let memo = MissMemo::new();
        assert!(!memo.hit("a"));
        memo.record("a");
        assert!(memo.hit("a"));
        assert!(!memo.hit("b"));
        memo.map
            .lock()
            .unwrap()
            .insert("old".into(), Instant::now() - NOT_FOUND_TTL * 2);
        assert!(!memo.hit("old"));
    }
}
