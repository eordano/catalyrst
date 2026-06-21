# E2E test plan — catalyrst-media (autotranslate)

Reimplementation of `autotranslate-server.decentraland.org` (key=`autotranslate`).

| Field | Value |
|---|---|
| Crate | `catalyrst-media` |
| Workspace | `<WORKSPACE>` |
| Local bind | `127.0.0.1:5143` (`HTTP_SERVER_HOST`/`HTTP_SERVER_PORT` in the service's environment file) |
| Backend | `mock` (echo, en/1.0) by default; `http` -> LibreTranslate via `TRANSLATE_BACKEND_URL`/`_API_KEY` |
| DB | content DB (`translation_cache` table, auto-migrated on boot) |
| Auth | none (endpoint is unauthenticated) |
| Upstream host | `https://autotranslate-server.decentraland.{ENV}/translate` (ENV = org|zone) |

Routes:
- `POST /translate` — single (`{q:string,...}`) and batch (`{q:string[],...}`) distinguished by the JSON type of `q` (serde untagged).
- `GET /ping` — echoes the request path (ops health).

---

## 1. Unity config — how to repoint

The translate URL is **hardcoded in Unity, NOT realm/`/about`-discovered.** It is resolved
purely from `DecentralandUrl.ChatTranslate` through the `RawUrl(...)` switch; the realm `/about`
response has no field that feeds this host. So you repoint it by editing Unity, not by editing our
`/about`.

**File (in your `unity-explorer` checkout):** `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`

**Line 226 (exact, current):**
```csharp
DecentralandUrl.ChatTranslate => $"https://autotranslate-server.decentraland.{ENV}/translate",
```

`{ENV}` (the `ENV` const, line 33) is a global string replace done at `RawUrl(...).Replace(ENV, decentralandDomain)`
(line 98) where `decentralandDomain` is `org` or `zone`. The enum is `DecentralandUrl.ChatTranslate = 66`
(`.../Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs:104`) and is consumed by
`DclTranslationProvider.translateUrl` (`.../DCL/Translation/Service/Provider/DclTranslationProvider.cs:20`).

**Repoint to your local service — replace line 226 with:**
```csharp
DecentralandUrl.ChatTranslate => "http://127.0.0.1:5143/translate",
```
(Drop the `$"..{ENV}.."` interpolation entirely so it is not rewritten per-environment. Note `http` not
`https` for the local node, and the path stays `/translate`.)

This is the only line to change. The client always POSTs to `Url(ChatTranslate)` for both single and
batch — there is no separate batch URL.

---

## 2. Concrete e2e checks (against local `:5143`)

### Start the service

```bash
set -a; source <ENV_FILE>; set +a
cd <WORKSPACE>
cargo run -p catalyrst-media
# migrations run automatically on boot via sqlx::migrate!; mock backend is the default.
```

### Check A — ping / health
```bash
curl -s -i http://127.0.0.1:5143/ping
```
Expect: `200 OK`, body is the literal echoed path `/ping`.

### Check B — single translate (mock echoes input, en/1.0)
The Unity client sends `{q, source:"auto", target, format:"text"}`. Mock backend returns the input unchanged.
```bash
curl -s -i -X POST http://127.0.0.1:5143/translate \
  -H 'content-type: application/json' \
  -d '{"q":"hello world","source":"auto","target":"es","format":"text"}'
```
Expect: `200`, body:
```json
{"detectedLanguage":{"confidence":1.0,"language":"en"},"translatedText":"hello world"}
```
Field names/casing must match Unity Newtonsoft binding exactly: `detectedLanguage{confidence,language}`, `translatedText`.

### Check C — batch translate (arrays, 1:1, order preserved)
```bash
curl -s -i -X POST http://127.0.0.1:5143/translate \
  -H 'content-type: application/json' \
  -d '{"q":["one","two","three"],"source":"auto","target":"es","format":"text"}'
```
Expect: `200`, `translatedText` is a length-3 array in input order `["one","two","three"]`, and
`detectedLanguage` is a length-3 array of `{confidence,language}`. `translatedText.length == q.length`
(guards the client's "Batch translation response size mismatch" exception).

### Check D — batch with intra-batch duplicates (dedup, still 1:1)
```bash
curl -s -i -X POST http://127.0.0.1:5143/translate \
  -H 'content-type: application/json' \
  -d '{"q":["dup","dup","unique"],"source":"auto","target":"fr","format":"text"}'
```
Expect: `200`, `translatedText == ["dup","dup","unique"]` (3 items — backend hit once for `dup`, reassembled 1:1).

### Check E — empty batch
```bash
curl -s -i -X POST http://127.0.0.1:5143/translate \
  -H 'content-type: application/json' \
  -d '{"q":[],"source":"auto","target":"es","format":"text"}'
```
Expect: `200`, body `{"detectedLanguage":[],"translatedText":[]}` (matches Unity's own empty-array short-circuit).

### Check F — malformed body -> 422
```bash
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:5143/translate \
  -H 'content-type: application/json' \
  -d '{"target":"es"}'
```
Expect: `422` (axum Json rejection — `q` matches neither String nor Vec<String>).

### Check G — bad JSON -> 4xx
```bash
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:5143/translate \
  -H 'content-type: application/json' -d 'not json'
```
Expect: `400`/`422` (JSON parse rejection, never `500`).

### Check H — cache persistence (DB side-effect)
After Checks B–D, confirm rows landed in the content DB:
```bash
psql "postgresql:///content?host=<SOCKET_DIR>&port=5433&user=<DB_USER>" \
  -c "SELECT backend, target_lang, detected_language, detected_confidence, left(translated_text,20) \
      FROM translation_cache ORDER BY created_at DESC LIMIT 10;"
```
Expect: rows with `backend='mock'`, the targets used above, `detected_language='en'`, `detected_confidence=1`.
Re-running Check B should serve from cache (no second backend call) and return the identical body.

### Optional — `http` backend (LibreTranslate) wiring
Only if exercising real translation: set `TRANSLATE_BACKEND=http`, `TRANSLATE_BACKEND_URL`,
optional `TRANSLATE_BACKEND_API_KEY`, restart, re-run Checks B/C. Each string is sent individually to
preserve the 1:1/order invariant; cached rows then carry `backend='http'`.

---

## 3. Real-client smoke (Unity is the only consumer)

`DecentralandUrl.ChatTranslate` exists **only in unity-explorer** — Bevy and Godot do not implement chat
translation, so `dcl-bevy` cannot exercise this host. Use the upstream Unity refclient via `dcl-walk`
(see the `dcl-explore` skill before driving it).

1. Apply the line-226 repoint above in your `unity-explorer` checkout (or in the refclient if it reads
   this source), pointing `ChatTranslate` at `http://127.0.0.1:5143/translate`.
2. Start `catalyrst-media` on `:5143` (mock backend is fine for the smoke).
3. `dcl-walk launch` then `dcl-walk auth-sign`; teleport into a scene with other speakers (or send chat).
4. Trigger chat auto-translate (open chat, enable translate / receive a message). The client POSTs to
   our `/translate`.
5. Verify: with the mock backend the translated text equals the source text (echo) and no
   "Batch translation response size mismatch" error appears in logs
   (`ReportCategory.TRANSLATE` lines — see `DclTranslationProvider`/`TranslationDebug`).
6. Confirm `translation_cache` gained rows (Check H query) corresponding to the messages translated.
7. Watch the service log for the per-request `tower_http` trace and `backend=mock` on the startup line.

A passing smoke = chat lines round-trip through `:5143`, the response binds cleanly to
`TranslationApiResponse`/`TranslationApiResponseBatch`, and `translatedText.length == q.length` holds for
batches.
