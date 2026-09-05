use async_trait::async_trait;

use super::{TranslatedItem, TranslationBackend};

pub struct MockBackend;

#[async_trait]
impl TranslationBackend for MockBackend {
    async fn translate(
        &self,
        texts: &[String],
        _source: &str,
        _target: &str,
        _format: &str,
    ) -> Result<Vec<TranslatedItem>, String> {
        Ok(texts
            .iter()
            .map(|t| TranslatedItem {
                translated_text: t.clone(),
                detected_language: "en".to_string(),
                detected_confidence: 1.0,
            })
            .collect())
    }
}
