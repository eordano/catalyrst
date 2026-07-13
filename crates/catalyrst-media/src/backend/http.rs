use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::{TranslatedItem, TranslationBackend};

pub struct HttpBackend {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct LtDetected {
    #[serde(default)]
    confidence: f32,
    #[serde(default)]
    language: String,
}

#[derive(Deserialize)]
struct LtResponse {
    #[serde(rename = "detectedLanguage")]
    detected_language: Option<LtDetected>,
    #[serde(rename = "translatedText")]
    translated_text: String,
}

impl HttpBackend {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client");
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    async fn translate_one(
        &self,
        text: &str,
        source: &str,
        target: &str,
        format: &str,
    ) -> Result<TranslatedItem, String> {
        let mut body = json!({
            "q": text,
            "source": source,
            "target": target,
            "format": format,
        });
        if let Some(key) = &self.api_key {
            body["api_key"] = json!(key);
        }
        let resp = self
            .client
            .post(format!("{}/translate", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            return Err(format!("upstream {status}: {txt}"));
        }
        let parsed: LtResponse = resp
            .json()
            .await
            .map_err(|e| format!("decode failed: {e}"))?;
        let (language, confidence) = match parsed.detected_language {
            Some(d) => (
                if d.language.is_empty() {
                    source.to_string()
                } else {
                    d.language
                },
                d.confidence,
            ),
            None => (source.to_string(), 0.0),
        };
        Ok(TranslatedItem {
            translated_text: parsed.translated_text,
            detected_language: language,
            detected_confidence: confidence,
        })
    }
}

#[async_trait]
impl TranslationBackend for HttpBackend {
    async fn translate(
        &self,
        texts: &[String],
        source: &str,
        target: &str,
        format: &str,
    ) -> Result<Vec<TranslatedItem>, String> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.translate_one(t, source, target, format).await?);
        }
        Ok(out)
    }
}
