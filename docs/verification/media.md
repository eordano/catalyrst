# Verification — catalyrst-media (service "media", autotranslate / LibreTranslate)

Branch: feat/service-plane-crates (committed tree). Analysis only; nothing run.

Crate: `crates/catalyrst-media`
Upstream: LibreTranslate-compatible `autotranslate-server.decentraland.{ENV}/translate` (upstream source absent from mirrors; verified against the Unity DTOs and the LibreTranslate `/translate` contract).
Unity consumer: `unity-explorer/Explorer/Assets/DCL/Translation/`
- DTOs: `Models/DTO/TranslationDTOs.cs`
- Provider: `Service/Provider/DclTranslationProvider.cs`
- URL wiring: `NetworkDefinitions/Browser/DecentralandUrlsSource.cs:226` (`DecentralandUrl.ChatTranslate = 66`)

Net-catalog confirms the endpoint is live, with BOTH call shapes:
- `POST .../translate  JSON:TranslationRequestBody{q,source,target,format}` (single)
- `POST .../translate  JSON:TranslationRequestBodyBatch{q[],source,target,format}` (batch)

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| POST /translate (single) | match | ok | none | partial | C# `TranslationApiResponse{detectedLanguage:DetectedLanguageDto, translatedText:string}` matches Rust `SingleResponse` exactly (camelCase via serde rename; inner `confidence:f32`/`language:string` match C# `float confidence`/`string language`). Untagged enum disambiguates single vs batch by `q` type. |
| POST /translate (batch) | match | ok | none | partial | C# `TranslationApiResponseBatch{detectedLanguage:DetectedLanguageDto[], translatedText:string[]}` matches Rust `BatchResponse`. Empty batch -> `{detectedLanguage:[],translatedText:[]}`, matching the client's own empty-array fast path (`DclTranslationProvider.cs:47-51`). Batch len-mismatch never returned (guarded -> 500/502). |
| GET /ping | n/a | not client-called | none | n/a | Standalone-only; excluded from `api_router()` and from the social bundle. Absent from net-catalog. |
| GET /health | n/a | not client-called | none | n/a | Standalone-only; bundle defines its own `/health`. Absent from net-catalog. |

`failure-modes-ok = partial` for /translate because the input-validation error path and the DB-hard-dependency path diverge from upstream (gaps below); all handler-level errors are coherent.

## Confirmed (re-check findings hold)

- **Shape parity is real and correct on the committed tree.** `handlers/translate.rs:47-74` defines `DetectedLanguageDto{confidence:f32, language:String}`, `SingleResponse` (serde-renamed `detectedLanguage`/`translatedText`), `BatchResponse`, and an `#[serde(untagged)]` response enum. These match the C# DTOs field-for-field. Not cosmetic, not stale, not fixed-away — a genuine match, so there is no shape divergence to report.
- **`detectedLanguage` is always a populated object, never null/omitted.** This is the key reason the latent C# null-deref does not fire. HTTP backend (`backend/http.rs:74-84`) maps a missing/`None` LibreTranslate `detectedLanguage` to `(source, 0.0)` and an empty `language` to `source`; mock (`backend/mock.rs`) emits `en`/`1.0`. The handler always constructs a `DetectedLanguageDto`. So our serialized single response always contains `detectedLanguage:{confidence,language}`.
- **Untagged-enum disambiguation works as claimed.** `SingleReq.q: String` fails to deserialize a JSON array, so a batch body falls through to `BatchReq.q: Vec<String>`; a single string body parses as `Single` (`handlers/translate.rs:13-38`).
- **Error model uniform for handler-level errors** (`http/errors.rs`): `BadRequest->400`, `Backend->502` (detail suppressed + logged), `Database->500 "database error"`, `Internal->500 "internal error"`, all as `{"error":<msg>}` JSON. Backend failures correctly degrade to 502, not 500.
- **Startup is panic-free under normal misconfig but Postgres is a hard boot dependency.** `Config::from_env` returns clean `anyhow::Err` for missing `MEDIA_PG_CONNECTION_STRING`, bad port, or `TRANSLATE_BACKEND=http` without `TRANSLATE_BACKEND_URL` (`config.rs:37-41,45`). `build_state` connects the pool AND runs `sqlx::migrate!`; an unreachable DB at boot returns `Err` (clean exit, not a panic) — `lib.rs:30-47`. The `.expect("backend url checked in config")` (`lib.rs:54`) is guarded by the config check so it cannot fire. `HttpBackend::new` `.expect("reqwest client")` (`http.rs:34`) only on pathological runtime/TLS init failure. No LiveKit in this crate.
- **Bundle mounting confirmed.** `catalyrst-social/src/main.rs:34,117-120` mounts media via `api_router().with_state(...)` only (just `/translate`); the bundle supplies its own `/health` and the standalone `/ping`+`/health` (`main.rs:24-25`) are NOT included. Standalone binary defaults to port 5157.

## Client-crash risks

- **Latent (not triggered by our service): single-path null-deref of `detectedLanguage`.** `DclTranslationProvider.cs:40-41` does `LanguageCodeParser.Parse(response.detectedLanguage.language)` with no null guard. If `detectedLanguage` were ever null/omitted, this would NPE. **Our Rust never emits a null/omitted `detectedLanguage`** (always a present object, per the HTTP/mock defaults above), so the crash cannot be triggered by catalyrst-media as written. Risk is real in the C# but unreachable against our output. The batch path is null-safe (`DclTranslationProvider.cs:57` checks `resp == null || resp.translatedText == null || length != texts.Length` and throws a caught exception, not an NPE).
- No request-throws crash beyond the standard one: the Unity `IWebRequestController` throws on any non-2xx regardless of body, which is expected/handled, not a crash.

## Failure-mode gaps (confirmed divergences)

1. **Malformed/invalid request body does NOT use the `{"error":...}` model.** The most likely real-world 4xx never reaches `ApiError`: axum's `Json<TranslateRequest>` extractor rejects it with a PLAIN-TEXT 400/422 body ("Failed to deserialize the JSON body..."). No custom `JsonRejection -> ApiError` handler is wired (`handlers/translate.rs:78` uses bare `Json(req)`; no rejection mapper in `lib.rs`/`errors.rs`). LibreTranslate returns 400 with `{"error":...}` JSON. **Severity: low** — the Unity client treats any non-2xx as a thrown exception irrespective of body shape, so the divergent body never reaches DTO parsing.
2. **Cache DB is a hard per-request dependency (main weakness).** `cache::fetch` / `cache::store` errors are `#[from] sqlx::Error -> ApiError::Database -> 500 "database error"` (`errors.rs:13,36-39`). A transient DB blip fails the WHOLE request even though the backend could still translate — `run()` (`handlers/translate.rs:139,182`) calls cache before and after the backend with `?`, no best-effort fallback. LibreTranslate has no DB and would still serve. **Severity: medium** (degradation gap, not a crash). The 500 here also slightly diverges from upstream, which never 500s on a translatable request.
3. **Oversized body** -> axum default 2MB limit -> 413 plain-text (default extractor), vs LibreTranslate's own char limit -> 400 JSON. Low/cosmetic.

All other listed failure modes are accurate: single backend returning zero items -> `Internal` 500 (guarded `translate.rs:83-86`); batch len mismatch -> 500 / `Backend` 502 (never a mismatched array, `translate.rs:103-108,169-175`); backend unreachable/non-2xx -> `Backend` 502 with detail logged + hidden (`http.rs:65-68`, `errors.rs:32-35`); no client auth on `/translate` (matches — `api_router` has no auth layer).

## Verdict

Re-check findings are ACCURATE. No false positives to reject. The shape verdict "match" is genuine (not a missed divergence). The single-path null-deref is a real latent C# fragility but is NOT reachable through our service because our output always carries a non-null `detectedLanguage`. The two substantive parity gaps are both failure-mode (not shape): plain-text JSON-rejection bodies (low) and the DB-as-hard-dependency 500 (medium). No client-crash is triggerable by catalyrst-media's responses.
