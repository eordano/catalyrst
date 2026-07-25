pub mod http;
pub mod mock;

use async_trait::async_trait;

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
