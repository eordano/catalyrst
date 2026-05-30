use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::backend::TranslatedItem;

pub fn hash_text(text: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    h.finalize().to_vec()
}

pub struct CachedRow {
    pub text_sha256: Vec<u8>,
    pub translated_text: String,
    pub detected_language: String,
    pub detected_confidence: f32,
}

pub async fn fetch(
    pool: &PgPool,
    backend: &str,
    target: &str,
    hashes: &[Vec<u8>],
) -> Result<Vec<CachedRow>, sqlx::Error> {
    if hashes.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<_, (Vec<u8>, String, String, f32)>(
        "SELECT text_sha256, translated_text, detected_language, detected_confidence \
         FROM translation_cache \
         WHERE backend = $1 AND target_lang = $2 AND text_sha256 = ANY($3)",
    )
    .bind(backend)
    .bind(target)
    .bind(hashes)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(text_sha256, translated_text, detected_language, detected_confidence)| CachedRow {
            text_sha256,
            translated_text,
            detected_language,
            detected_confidence,
        })
        .collect())
}

pub async fn store(
    pool: &PgPool,
    backend: &str,
    target: &str,
    rows: &[(Vec<u8>, &TranslatedItem)],
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }
    let hashes: Vec<Vec<u8>> = rows.iter().map(|(h, _)| h.clone()).collect();
    let translated: Vec<String> = rows.iter().map(|(_, i)| i.translated_text.clone()).collect();
    let languages: Vec<String> = rows.iter().map(|(_, i)| i.detected_language.clone()).collect();
    let confidences: Vec<f32> = rows.iter().map(|(_, i)| i.detected_confidence).collect();
    sqlx::query(
        "INSERT INTO translation_cache \
           (backend, target_lang, text_sha256, translated_text, detected_language, detected_confidence) \
         SELECT $1, $2, h, t, l, c \
         FROM UNNEST($3::bytea[], $4::text[], $5::text[], $6::real[]) AS u(h, t, l, c) \
         ON CONFLICT (backend, target_lang, text_sha256) DO UPDATE SET \
           translated_text = EXCLUDED.translated_text, \
           detected_language = EXCLUDED.detected_language, \
           detected_confidence = EXCLUDED.detected_confidence",
    )
    .bind(backend)
    .bind(target)
    .bind(&hashes)
    .bind(&translated)
    .bind(&languages)
    .bind(&confidences)
    .execute(pool)
    .await?;
    Ok(())
}
