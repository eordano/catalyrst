# Parity report — catalyrst-media (service "media" / autotranslate)

Crate: `crates/catalyrst-media`
Upstream: `decentraland/autotranslate-server` (LibreTranslate-compatible `/translate`).
Unity client surface: `POST https://autotranslate-server.decentraland.{ENV}/translate`
(net-catalog: both `TranslationRequestBody{q,...}` and `TranslationRequestBodyBatch{q[],...}`).

Live diff: not-applicable (upstream autotranslate-server is not running locally; it is also
**not present in the mirror clones** — referenced only by `architecture/architecture.dot:147`
and `unity-explorer/.../DecentralandUrlsSource.cs:226`). Upstream internals were therefore
verified only insofar as source is available; see the rejected/caveats section.

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| POST /translate | match | better (qualified) | none | Field-for-field match to the Unity DTOs for both single and batch. Cache win is real for the repeat-translation case but the cold path is not optimal and the "upstream has no cache" premise is only partly verifiable. |
| GET /ping | n/a (no upstream) | same | none | Liveness probe only; not client-facing, not in net-catalog, excluded from `api_router()`. |

## Confirmed shape parity (POST /translate)

Verified directly against the Unity DTOs in
`unity-explorer/Explorer/Assets/DCL/Translation/Models/DTO/TranslationDTOs.cs` and the
provider in `.../Service/Provider/DclTranslationProvider.cs`:

- **Single** — Rust `SingleResponse` (`translate.rs:58-64`) serializes
  `detected_language -> detectedLanguage` and `translated_text -> translatedText` via explicit
  `#[serde(rename)]`; nested `DetectedLanguageDto{confidence:f32, language:String}`
  (`translate.rs:51-55`) uses bare lowercase names. Matches `TranslationApiResponse` +
  `DetectedLanguageDto` (`TranslationDTOs.cs:24-42`) field-for-field.
- **Batch** — Rust `BatchResponse` (`translate.rs:67-73`) uses `Vec<DetectedLanguageDto>` +
  `Vec<String>`, matching `TranslationApiResponseBatch{ DetectedLanguageDto[]; string[] }`
  (`TranslationDTOs.cs:30-35`).
- **Type fidelity** — `confidence` is `f32` -> JSON number; Unity reads `float confidence`
  (`TranslationDTOs.cs:40`). No string-vs-number drift.
- **What the client actually reads** — confirmed the field is not ignored: the single path reads
  `response.detectedLanguage.language` (`DclTranslationProvider.cs:40`). The batch path
  deliberately ignores languages ("Ignore languages completely", line 56) but still requires the
  arrays to be present and `translatedText.length == texts.length` (line 57-58). The Rust handler
  enforces that exact invariant (`translate.rs:111-117`) and returns parallel empty arrays — not
  null — for an empty batch (`translate.rs:102-107`), matching the client's own empty-batch
  short-circuit (`DclTranslationProvider.cs:47-51`). No null-vs-omitted drift.
- **Discrimination** — `serde(untagged)` on `TranslateResponse`/`TranslateRequest` selects the
  variant by JSON shape (string vs array); the client picks the DTO the same way by what it sent.
  Symmetric. No `{data:[...]}` wrapper, no pagination envelope on either side.

No shape issues found. Shape verdict **match** is upheld.

## Confirmed efficiency win (POST /translate) — qualified "better"

Structural reason verified in source:

- **Persistent Postgres cache exists and is real.** `migrations/0001_translation_cache.sql`
  defines `translation_cache` keyed on `(backend, target_lang, text_sha256)`. The read lane is a
  single batched query covering every input in one round-trip —
  `... WHERE backend=$1 AND target_lang=$2 AND text_sha256 = ANY($3)` (`cache.rs:31-40`) — plus
  in-batch dedup of identical strings (`translate.rs:165-172`). A fully-cached batch therefore
  costs exactly **1 DB read and 0 backend calls**. This is the common chat case (repeated
  phrases), so it is a genuine structural advantage over a per-request inference proxy.
- **Cold-path caveat (kept the verdict from being a blowout, confirmed accurate).**
  `cache::store` loops N single-row `INSERT ... ON CONFLICT` statements (`cache.rs:59-77`), not one
  multi-row insert. `HttpBackend::translate` issues one sequential `POST /translate` per miss
  string (`http.rs:109-113`, `translate_one` at `http.rs:49-97`) — it never batches `q[]` upstream.
  So M misses = M sequential upstream POSTs + M INSERTs. The "better" verdict's cold-path caveat is
  accurate and load-bearing.

Net: **better for the cache-hit path**, which dominates in practice, so "better" stands — but
only with the cold-path caveat attached.

## Rejected / downgraded during verification

- **Premise "upstream autotranslate-server is stateless per-request with no cache" — NOT fully
  verifiable, partially contradicted by the crate's own comment.** `decentraland/autotranslate-server`
  source is absent from the mirrors (only the GitHub URL appears, in `architecture.dot:147`), so I
  could not confirm upstream has no caching layer by reading it. The "better" claim leans on
  LibreTranslate being a stateless inference server (true of vanilla LibreTranslate), but the
  crate's own migration header says "Postgres replaces autotranslate-server's caching layer"
  (`0001_translation_cache.sql:1`), which implies upstream *does* have some caching. The
  structural distinction (cache lives on our proxy, batched + deduped; vs. wherever upstream's
  cache lives) is still a real difference, but the efficiency win is **NOT** a clean "upstream is
  cacheless" win. Verdict kept at "better" but explicitly **qualified**, not the categorical claim
  the rationale originally implied.
- **No claim was rejected on language-choice grounds** — the win is genuinely structural (the
  persistent batched/deduped cache), not "Rust is faster than TS/Python."

## /ping

Confirmed: `ping.rs:4-6` returns `OriginalUri.path()` as a bare `text/plain` string — no JSON, no
schema. Registered only on the standalone binary (`main.rs:24`) and deliberately excluded from
`api_router()` (`lib.rs:70-74`) to avoid a path collision when merged into the social bundle. Not
in the net-catalog; not a client-facing endpoint. Efficiency "same", severity "none" — upheld.
