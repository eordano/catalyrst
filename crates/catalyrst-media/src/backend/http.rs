use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::{TranslatedItem, TranslationBackend};

#[derive(Clone)]
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

enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for OneOrMany<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value.is_array() {
            serde_json::from_value(value).map(Self::Many)
        } else {
            serde_json::from_value(value).map(Self::One)
        }
        .map_err(serde::de::Error::custom)
    }
}

/// LibreTranslate answers a string `q` with scalars and an array `q` with arrays.
#[derive(Deserialize)]
struct LtResponse {
    #[serde(rename = "detectedLanguage")]
    detected_language: Option<OneOrMany<LtDetected>>,
    #[serde(rename = "translatedText")]
    translated_text: OneOrMany<String>,
}

fn into_items(parsed: LtResponse, n: usize, source: &str) -> Result<Vec<TranslatedItem>, String> {
    let texts = match parsed.translated_text {
        OneOrMany::One(s) => vec![s],
        OneOrMany::Many(v) => v,
    };
    if texts.len() != n {
        return Err(format!(
            "upstream returned {} translations for {n} texts",
            texts.len()
        ));
    }
    let mut detected: Vec<Option<LtDetected>> = match parsed.detected_language {
        Some(OneOrMany::Many(v)) => v.into_iter().map(Some).collect(),
        Some(OneOrMany::One(d)) => vec![Some(d)],
        None => Vec::new(),
    };
    detected.resize_with(n, || None);
    Ok(texts
        .into_iter()
        .zip(detected)
        .map(|(translated_text, d)| {
            let (language, confidence) = match d {
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
            TranslatedItem {
                translated_text,
                detected_language: language,
                detected_confidence: confidence,
            }
        })
        .collect())
}

enum BatchFailure {
    /// The upstream rejected or misread the array form; per-text requests still apply.
    Unsupported(String),
    Fatal(String),
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

    async fn post(
        &self,
        q: serde_json::Value,
        source: &str,
        target: &str,
        format: &str,
    ) -> Result<reqwest::Response, String> {
        let mut body = json!({
            "q": q,
            "source": source,
            "target": target,
            "format": format,
        });
        if let Some(key) = &self.api_key {
            body["api_key"] = json!(key);
        }
        self.client
            .post(format!("{}/translate", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))
    }

    async fn translate_one(
        &self,
        text: &str,
        source: &str,
        target: &str,
        format: &str,
    ) -> Result<TranslatedItem, String> {
        let resp = self.post(json!(text), source, target, format).await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            return Err(format!("upstream {status}: {txt}"));
        }
        let parsed: LtResponse = resp
            .json()
            .await
            .map_err(|e| format!("decode failed: {e}"))?;
        into_items(parsed, 1, source).map(|mut v| v.remove(0))
    }

    async fn translate_batch(
        &self,
        texts: &[String],
        source: &str,
        target: &str,
        format: &str,
    ) -> Result<Vec<TranslatedItem>, BatchFailure> {
        let resp = self
            .post(json!(texts), source, target, format)
            .await
            .map_err(BatchFailure::Fatal)?;
        let status = resp.status();
        if !status.is_success() {
            let txt = resp.text().await.unwrap_or_default();
            let msg = format!("upstream {status}: {txt}");
            return Err(if status.is_client_error() {
                BatchFailure::Unsupported(msg)
            } else {
                BatchFailure::Fatal(msg)
            });
        }
        let parsed: LtResponse = resp
            .json()
            .await
            .map_err(|e| BatchFailure::Unsupported(format!("decode failed: {e}")))?;
        into_items(parsed, texts.len(), source).map_err(BatchFailure::Unsupported)
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
        match texts {
            [] => return Ok(Vec::new()),
            [text] => {
                return Ok(vec![
                    self.translate_one(text, source, target, format).await?,
                ])
            }
            _ => {}
        }
        match self.translate_batch(texts, source, target, format).await {
            Ok(items) => return Ok(items),
            Err(BatchFailure::Fatal(e)) => return Err(e),
            Err(BatchFailure::Unsupported(e)) => {
                tracing::debug!(error = %e, "batched translate unsupported; sending per text");
            }
        }
        let (source, target, format) = (source.to_string(), target.to_string(), format.to_string());
        super::translate_concurrently(texts, |text| {
            let backend = self.clone();
            let (source, target, format) = (source.clone(), target.clone(), format.clone());
            async move {
                backend
                    .translate_one(&text, &source, &target, &format)
                    .await
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::Value;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    async fn spawn(handler: axum::routing::MethodRouter) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route("/translate", handler);
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    fn texts() -> Vec<String> {
        vec!["one".to_string(), "two".to_string(), "three".to_string()]
    }

    #[tokio::test]
    async fn batches_many_texts_into_one_request() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let addr = spawn(post(move |Json(body): Json<Value>| {
            let seen = seen.clone();
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                let q: Vec<String> = serde_json::from_value(body["q"].clone()).unwrap();
                let translated: Vec<String> = q.iter().map(|t| format!("{t}!")).collect();
                let detected: Vec<Value> = q
                    .iter()
                    .map(|_| serde_json::json!({ "language": "en", "confidence": 0.5 }))
                    .collect();
                Json(serde_json::json!({ "translatedText": translated, "detectedLanguage": detected }))
            }
        }))
        .await;
        let backend = HttpBackend::new(format!("http://{addr}"), None);
        let out = backend
            .translate(&texts(), "auto", "es", "text")
            .await
            .unwrap();
        let got: Vec<&str> = out.iter().map(|i| i.translated_text.as_str()).collect();
        assert_eq!(got, vec!["one!", "two!", "three!"]);
        assert_eq!(out[1].detected_language, "en");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn falls_back_to_per_text_when_arrays_are_rejected() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let addr = spawn(post(move |Json(body): Json<Value>| {
            let seen = seen.clone();
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                match body["q"].as_str() {
                    Some(t) => (
                        axum::http::StatusCode::OK,
                        Json(serde_json::json!({ "translatedText": format!("{t}!") })),
                    ),
                    None => (
                        axum::http::StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": "q must be a string" })),
                    ),
                }
            }
        }))
        .await;
        let backend = HttpBackend::new(format!("http://{addr}"), None);
        let out = backend
            .translate(&texts(), "auto", "es", "text")
            .await
            .unwrap();
        let got: Vec<&str> = out.iter().map(|i| i.translated_text.as_str()).collect();
        assert_eq!(got, vec!["one!", "two!", "three!"]);
        assert_eq!(out[0].detected_language, "auto");
        assert_eq!(calls.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn upstream_server_errors_are_not_retried_per_text() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let addr = spawn(post(move || {
            let seen = seen.clone();
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                (axum::http::StatusCode::BAD_GATEWAY, "down")
            }
        }))
        .await;
        let backend = HttpBackend::new(format!("http://{addr}"), None);
        let err = backend
            .translate(&texts(), "auto", "es", "text")
            .await
            .unwrap_err();
        assert!(err.contains("502"), "{err}");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn shape_mismatch_is_an_error() {
        let parsed: LtResponse = serde_json::from_str(r#"{"translatedText":["a","b"]}"#).unwrap();
        assert!(into_items(parsed, 3, "auto").is_err());
        let parsed: LtResponse = serde_json::from_str(r#"{"translatedText":"a"}"#).unwrap();
        let items = into_items(parsed, 1, "fr").unwrap();
        assert_eq!(items[0].detected_language, "fr");
    }
}
