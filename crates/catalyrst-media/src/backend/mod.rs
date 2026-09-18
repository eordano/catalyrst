pub mod http;
pub mod llm;
pub mod mock;

use std::sync::Arc;

use async_trait::async_trait;

use crate::config::{BackendKind, Config};

#[derive(Debug, Clone)]
pub struct TranslatedItem {
    pub translated_text: String,
    pub detected_language: String,
    pub detected_confidence: f32,
}

#[async_trait]
pub trait TranslationBackend: Send + Sync {
    async fn translate(
        &self,
        texts: &[String],
        source: &str,
        target: &str,
        format: &str,
    ) -> Result<Vec<TranslatedItem>, String>;
}

pub fn build_backend(cfg: &Config) -> Arc<dyn TranslationBackend> {
    match cfg.backend_kind {
        BackendKind::Mock => Arc::new(mock::MockBackend),
        BackendKind::Http => Arc::new(http::HttpBackend::new(
            cfg.backend_url
                .clone()
                .expect("http backend url checked in config"),
            cfg.backend_api_key.clone(),
        )),
        BackendKind::Llm => Arc::new(llm::LlmBackend::new(
            cfg.llm_base_url
                .clone()
                .expect("llm base url checked in config"),
            cfg.llm_api_key.clone(),
            cfg.llm_model.clone(),
        )),
    }
}

pub(crate) const TRANSLATE_CONCURRENCY: usize = 8;

/// Maps `f` over `texts` with at most TRANSLATE_CONCURRENCY in flight, in input order;
/// the first error aborts the rest, as the sequential loop did.
pub(crate) async fn translate_concurrently<F, Fut>(
    texts: &[String],
    f: F,
) -> Result<Vec<TranslatedItem>, String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<TranslatedItem, String>> + Send + 'static,
{
    let semaphore = Arc::new(tokio::sync::Semaphore::new(TRANSLATE_CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();
    let mut out: Vec<Option<TranslatedItem>> = (0..texts.len()).map(|_| None).collect();
    let settle =
        |joined: Result<(usize, Result<TranslatedItem, String>), tokio::task::JoinError>,
         out: &mut Vec<Option<TranslatedItem>>|
         -> Result<(), String> {
            let (i, item) = joined.map_err(|e| format!("translate task failed: {e}"))?;
            out[i] = Some(item?);
            Ok(())
        };
    for (i, text) in texts.iter().cloned().enumerate() {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        while let Some(joined) = tasks.try_join_next() {
            settle(joined, &mut out)?;
        }
        let fut = f(text);
        tasks.spawn(async move {
            let _permit = permit;
            (i, fut.await)
        });
    }
    while let Some(joined) = tasks.join_next().await {
        settle(joined, &mut out)?;
    }
    Ok(out.into_iter().flatten().collect())
}
