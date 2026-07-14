use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{TranslatedItem, TranslationBackend};

// Adapter over an OpenAI-compatible chat-completions proxy (llm.decent.dev). Each translate
// request becomes one /v1/chat/completions call whose reply is a JSON object we map back onto
// the LibreTranslate-shaped TranslatedItem the rest of the crate speaks.
pub struct LlmBackend {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct LlmTranslatePayload {
    #[serde(default)]
    translated_text: String,
    #[serde(default)]
    detected_language: String,
    #[serde(default)]
    confidence: f32,
}

impl LlmBackend {
    pub fn new(base_url: String, api_key: Option<String>, model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client");
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
        }
    }

    fn system_prompt(source: &str, target: &str, format: &str) -> String {
        let mut p = format!(
            "You are a professional translation engine. Translate the user's message into the \
             language identified by the ISO 639-1 code '{target}'. Respond with ONLY a compact \
             JSON object, no markdown fences, with exactly these keys: \"translated_text\" (the \
             translation as a string), \"detected_language\" (ISO 639-1 code of the source \
             language), \"confidence\" (a number between 0 and 1 for how confident the source \
             detection is). Preserve meaning; do not add commentary."
        );
        let source = source.trim();
        if !source.is_empty() && !source.eq_ignore_ascii_case("auto") {
            p.push_str(&format!(" The source language is '{source}'."));
        }
        if format.eq_ignore_ascii_case("html") {
            p.push_str(
                " The input is HTML: keep every tag and attribute intact and translate only the \
                 human-readable text.",
            );
        }
        p
    }

    async fn translate_one(
        &self,
        text: &str,
        source: &str,
        target: &str,
        format: &str,
    ) -> Result<TranslatedItem, String> {
        let body = json!({
            "model": self.model,
            "temperature": 0,
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": Self::system_prompt(source, target, format) },
                { "role": "user", "content": text },
            ],
        });
        let mut req = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            return Err(format!("llm proxy {status}: {txt}"));
        }
        let parsed: ChatResponse = resp
            .json()
            .await
            .map_err(|e| format!("decode failed: {e}"))?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| "llm proxy returned no choices".to_string())?;
        Ok(map_completion(&content, source))
    }
}

fn strip_json_fences(s: &str) -> &str {
    let t = s.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    t.trim().trim_end_matches("```").trim()
}

// The proxy usually honors the JSON instruction, but a stray model may return either bare prose
// or a fenced blob. Parse the structured payload when present; otherwise fall back to treating the
// whole reply as the translation so we surface a real (if unlabeled) result rather than an error.
fn map_completion(content: &str, source: &str) -> TranslatedItem {
    let body = strip_json_fences(content);
    if let Ok(payload) = serde_json::from_str::<LlmTranslatePayload>(body) {
        if !payload.translated_text.is_empty() {
            let detected = if payload.detected_language.trim().is_empty() {
                source.to_string()
            } else {
                payload.detected_language
            };
            return TranslatedItem {
                translated_text: payload.translated_text,
                detected_language: detected,
                detected_confidence: payload.confidence,
            };
        }
    }
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(body) {
        if let Some(t) = map.get("translated_text").and_then(Value::as_str) {
            return TranslatedItem {
                translated_text: t.to_string(),
                detected_language: source.to_string(),
                detected_confidence: 0.0,
            };
        }
    }
    TranslatedItem {
        translated_text: content.trim().to_string(),
        detected_language: source.to_string(),
        detected_confidence: 0.0,
    }
}

#[async_trait]
impl TranslationBackend for LlmBackend {
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::Value;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    async fn spawn(handler: axum::routing::MethodRouter) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route("/v1/chat/completions", handler);
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    fn completion(content: &str) -> Json<Value> {
        Json(json!({ "choices": [ { "message": { "role": "assistant", "content": content } } ] }))
    }

    #[tokio::test]
    async fn parses_structured_json_completion() {
        let addr = spawn(post(|| async {
            completion(
                "{\"translated_text\":\"hola\",\"detected_language\":\"en\",\"confidence\":0.97}",
            )
        }))
        .await;
        let backend = LlmBackend::new(format!("http://{addr}"), None, "test-model".into());
        let out = backend
            .translate(&["hello".to_string()], "auto", "es", "text")
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].translated_text, "hola");
        assert_eq!(out[0].detected_language, "en");
        assert!((out[0].detected_confidence - 0.97).abs() < 1e-6);
    }

    #[tokio::test]
    async fn strips_code_fences_and_defaults_language_to_source() {
        let addr = spawn(post(|| async {
            completion("```json\n{\"translated_text\":\"bonjour\"}\n```")
        }))
        .await;
        let backend = LlmBackend::new(format!("http://{addr}"), None, "test-model".into());
        let out = backend
            .translate(&["hello".to_string()], "en", "fr", "text")
            .await
            .unwrap();
        assert_eq!(out[0].translated_text, "bonjour");
        assert_eq!(out[0].detected_language, "en");
    }

    #[tokio::test]
    async fn falls_back_to_raw_content_when_not_json() {
        let addr = spawn(post(|| async { completion("hallo welt") })).await;
        let backend = LlmBackend::new(format!("http://{addr}"), None, "test-model".into());
        let out = backend
            .translate(&["hello world".to_string()], "en", "de", "text")
            .await
            .unwrap();
        assert_eq!(out[0].translated_text, "hallo welt");
        assert_eq!(out[0].detected_language, "en");
    }

    type SeenRequest = Arc<Mutex<Option<(Option<String>, Value)>>>;

    #[tokio::test]
    async fn forwards_bearer_key_and_prompt() {
        let seen: SeenRequest = Arc::new(Mutex::new(None));
        let sink = seen.clone();
        let addr = spawn(post(move |headers: axum::http::HeaderMap, Json(b): Json<Value>| {
            let sink = sink.clone();
            async move {
                let auth = headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                *sink.lock().unwrap() = Some((auth, b));
                completion("{\"translated_text\":\"ok\",\"detected_language\":\"en\",\"confidence\":1}")
            }
        }))
        .await;
        let backend = LlmBackend::new(
            format!("http://{addr}"),
            Some("secret-key".into()),
            "m".into(),
        );
        backend
            .translate(&["hi".to_string()], "auto", "ja", "html")
            .await
            .unwrap();
        let guard = seen.lock().unwrap();
        let (auth, body) = guard.as_ref().unwrap();
        assert_eq!(auth.as_deref(), Some("Bearer secret-key"));
        assert_eq!(body["model"], "m");
        let system = body["messages"][0]["content"].as_str().unwrap();
        assert!(system.contains("'ja'"));
        assert!(system.contains("HTML"));
    }

    #[tokio::test]
    async fn upstream_error_is_surfaced() {
        let backend = LlmBackend::new("http://127.0.0.1:1".into(), None, "m".into());
        let err = backend
            .translate(&["x".to_string()], "auto", "es", "text")
            .await
            .unwrap_err();
        assert!(err.contains("request failed"));
    }
}
