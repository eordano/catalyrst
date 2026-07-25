-- autotranslate cache. Postgres replaces autotranslate-server's caching layer.
-- One row per (backend, target_lang, text_sha256) — i.e. a unique source string
-- translated into a given target language by a given backend.
--
-- text_sha256        = sha256(q) over the UTF-8 source string. bytea (32 bytes).
-- target_lang        = lowercase ISO code the explorer requested (source is always "auto").
-- backend            = which TranslationBackend produced the row ("mock" | "http").
-- translated_text    = the translatedText value returned to the client.
-- detected_language  = detectedLanguage.language for this string.
-- detected_confidence= detectedLanguage.confidence for this string.
CREATE TABLE IF NOT EXISTS translation_cache (
    backend             text        NOT NULL,
    target_lang         text        NOT NULL,
    text_sha256         bytea       NOT NULL,
    translated_text     text        NOT NULL,
    detected_language   text        NOT NULL DEFAULT 'en',
    detected_confidence real        NOT NULL DEFAULT 1.0,
    created_at          timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (backend, target_lang, text_sha256)
);
