use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::backend::TranslatedItem;
use crate::cache;
use crate::http::{ApiError, JsonBody};
use crate::AppStateInner;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TranslateRequest {
    Single(SingleReq),
    Batch(BatchReq),
}

#[derive(Debug, Deserialize)]
pub struct SingleReq {
    pub q: String,
    #[serde(default = "default_source")]
    pub source: String,
    pub target: String,
    #[serde(default = "default_format")]
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct BatchReq {
    pub q: Vec<String>,
    #[serde(default = "default_source")]
    pub source: String,
    pub target: String,
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_source() -> String {
    "auto".to_string()
}
fn default_format() -> String {
    "text".to_string()
}

fn resolve_language(detected: String, requested_source: &str) -> String {
    let detected = detected.trim();
    if !detected.is_empty() && !detected.eq_ignore_ascii_case("auto") {
        return detected.to_string();
    }
    let requested = requested_source.trim();
    if !requested.is_empty() && !requested.eq_ignore_ascii_case("auto") {
        return requested.to_string();
    }
    "en".to_string()
}

fn enforce_limits(char_limit: usize, batch_limit: usize, texts: &[String]) -> Result<(), ApiError> {
    if texts.len() > batch_limit {
        return Err(ApiError::bad_request(format!(
            "batch size ({}) exceeds limit ({batch_limit})",
            texts.len()
        )));
    }
    let total_chars: usize = texts.iter().map(|t| t.chars().count()).sum();
    if total_chars > char_limit {
        return Err(ApiError::bad_request(format!(
            "request ({total_chars} characters) exceeds text limit ({char_limit})"
        )));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct DetectedLanguageDto {
    pub confidence: f32,
    pub language: String,
}

#[derive(Debug, Serialize)]
pub struct SingleResponse {
    #[serde(rename = "detectedLanguage")]
    pub detected_language: DetectedLanguageDto,
    #[serde(rename = "translatedText")]
    pub translated_text: String,
}

#[derive(Debug, Serialize)]
pub struct BatchResponse {
    #[serde(rename = "detectedLanguage")]
    pub detected_language: Vec<DetectedLanguageDto>,
    #[serde(rename = "translatedText")]
    pub translated_text: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum TranslateResponse {
    Single(SingleResponse),
    Batch(BatchResponse),
}

pub async fn translate(
    State(state): State<Arc<AppStateInner>>,
    JsonBody(req): JsonBody<TranslateRequest>,
) -> Result<Json<TranslateResponse>, ApiError> {
    match req {
        TranslateRequest::Single(s) => {
            enforce_limits(
                state.translate_char_limit,
                state.translate_batch_limit,
                std::slice::from_ref(&s.q),
            )?;
            let items = run(&state, &[s.q], &s.source, &s.target, &s.format).await?;
            let item = items
                .into_iter()
                .next()
                .ok_or_else(|| ApiError::Internal("backend returned no result".into()))?;
            Ok(Json(TranslateResponse::Single(SingleResponse {
                detected_language: DetectedLanguageDto {
                    confidence: item.detected_confidence,
                    language: resolve_language(item.detected_language, &s.source),
                },
                translated_text: item.translated_text,
            })))
        }
        TranslateRequest::Batch(b) => {
            if b.q.is_empty() {
                return Ok(Json(TranslateResponse::Batch(BatchResponse {
                    detected_language: Vec::new(),
                    translated_text: Vec::new(),
                })));
            }
            enforce_limits(
                state.translate_char_limit,
                state.translate_batch_limit,
                &b.q,
            )?;
            let items = run(&state, &b.q, &b.source, &b.target, &b.format).await?;
            if items.len() != b.q.len() {
                return Err(ApiError::Internal(format!(
                    "batch size mismatch: got {} for {} inputs",
                    items.len(),
                    b.q.len()
                )));
            }
            let mut detected_language = Vec::with_capacity(items.len());
            let mut translated_text = Vec::with_capacity(items.len());
            for item in items {
                detected_language.push(DetectedLanguageDto {
                    confidence: item.detected_confidence,
                    language: resolve_language(item.detected_language, &b.source),
                });
                translated_text.push(item.translated_text);
            }
            Ok(Json(TranslateResponse::Batch(BatchResponse {
                detected_language,
                translated_text,
            })))
        }
    }
}

async fn run(
    state: &AppStateInner,
    texts: &[String],
    source: &str,
    target: &str,
    format: &str,
) -> Result<Vec<TranslatedItem>, ApiError> {
    let target = target.to_lowercase();
    let backend_label = state.backend_label;

    let hashes: Vec<Vec<u8>> = texts.iter().map(|t| cache::hash_text(t)).collect();

    let cached = match cache::fetch(&state.pool, backend_label, &target, &hashes).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "translation cache fetch failed; serving without cache");
            Vec::new()
        }
    };
    let mut by_hash: HashMap<Vec<u8>, TranslatedItem> = cached
        .into_iter()
        .map(|r| {
            (
                r.text_sha256,
                TranslatedItem {
                    translated_text: r.translated_text,
                    detected_language: r.detected_language,
                    detected_confidence: r.detected_confidence,
                },
            )
        })
        .collect();

    let mut miss_indices: Vec<usize> = Vec::new();
    let mut seen_miss_hashes: HashMap<Vec<u8>, ()> = HashMap::new();
    for (i, h) in hashes.iter().enumerate() {
        if !by_hash.contains_key(h) && seen_miss_hashes.insert(h.clone(), ()).is_none() {
            miss_indices.push(i);
        }
    }

    if !miss_indices.is_empty() {
        let miss_texts: Vec<String> = miss_indices.iter().map(|&i| texts[i].clone()).collect();
        let translated = tokio::time::timeout(
            state.translate_timeout,
            state
                .backend
                .translate(&miss_texts, source, &target, format),
        )
        .await
        .map_err(|_| {
            let msg = format!(
                "translation timed out after {}s",
                state.translate_timeout.as_secs()
            );
            tracing::error!(error = %msg, "translation backend error");
            ApiError::http(502, "translation backend error")
        })?
        .map_err(|e| {
            tracing::error!(error = %e, "translation backend error");
            ApiError::http(502, "translation backend error")
        })?;
        if translated.len() != miss_texts.len() {
            let msg = format!(
                "backend returned {} items for {} inputs",
                translated.len(),
                miss_texts.len()
            );
            tracing::error!(error = %msg, "translation backend error");
            return Err(ApiError::http(502, "translation backend error"));
        }
        let mut to_store: Vec<(Vec<u8>, &TranslatedItem)> = Vec::with_capacity(translated.len());
        for (k, &i) in miss_indices.iter().enumerate() {
            let h = hashes[i].clone();
            to_store.push((h.clone(), &translated[k]));
            by_hash.insert(h, translated[k].clone());
        }
        if let Err(e) = cache::store(&state.pool, backend_label, &target, &to_store).await {
            tracing::warn!(error = %e, "translation cache store failed; result still served");
        }
    }

    let mut out = Vec::with_capacity(texts.len());
    for h in &hashes {
        let item = by_hash
            .get(h)
            .cloned()
            .ok_or_else(|| ApiError::Internal("missing translation after fill".into()))?;
        out.push(item);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DEFAULT_TRANSLATE_BATCH_LIMIT, DEFAULT_TRANSLATE_CHAR_LIMIT};

    fn texts(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn under_both_limits_passes() {
        assert!(enforce_limits(5000, 100, &texts(&["hello", "world"])).is_ok());
    }

    #[test]
    fn batch_over_limit_is_rejected() {
        let batch: Vec<String> = (0..3).map(|i| format!("t{i}")).collect();
        let err = enforce_limits(5000, 2, &batch).unwrap_err();
        assert!(
            matches!(err, ApiError::Http { status: 400, message } if message.contains("batch size (3)"))
        );
    }

    #[test]
    fn chars_are_summed_across_the_batch() {
        let batch = texts(&["aaaa", "bbbb"]);
        let err = enforce_limits(6, 100, &batch).unwrap_err();
        assert!(
            matches!(err, ApiError::Http { status: 400, message } if message.contains("8 characters"))
        );
        assert!(enforce_limits(8, 100, &batch).is_ok());
    }

    #[test]
    fn chars_counted_as_scalars_not_bytes() {
        assert!(enforce_limits(4, 100, &texts(&["\u{65e5}\u{672c}\u{8a9e}\u{3060}"])).is_ok());
        assert!(enforce_limits(3, 100, &texts(&["\u{65e5}\u{672c}\u{8a9e}\u{3060}"])).is_err());
    }

    #[test]
    fn defaults_match_libretranslate_shape() {
        assert_eq!(DEFAULT_TRANSLATE_CHAR_LIMIT, 5000);
        assert_eq!(DEFAULT_TRANSLATE_BATCH_LIMIT, 100);
    }
}
