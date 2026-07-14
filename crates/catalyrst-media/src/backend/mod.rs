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

// Config::from_env has already enforced that http/llm carry their required URL, so the unwraps
// here cannot fire; mock stays the fail-closed default when nothing is configured.
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
