---
id: landings-cast-not-found
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Surfacing a clear, recoverable 404 fallback for dead Cast 2.0 links (Go Home
    + View Documentation) -- and instrumenting it by where the visitor came from
    (streamer vs watcher) and why the link failed (missing / malformed / expired
    / ended) -- lets us size and route the dead-link funnel leak instead of losing
    those arrivals silently.
  because: >-
    Stream links are minted on-demand by the Admin Smart Item in a scene, so a
    bare /cast or a stale /cast/:token has nothing to resolve and dumps the
    visitor on a 404. Today that leak is invisible: we cannot tell whether dead
    links hit streamers (mis-shared their own console URL) or watchers (followed
    an expired/ended share link), nor whether anyone recovers via the CTAs.
    Counting the fallback by { from, reason } and counting CTA recoveries makes
    the leak measurable and points at the upstream fix (link minting / sharing).
metric:
  primary: cast_not_found_recovery_rate
  numerator: cast_not_found_go_home
  denominator: cast_not_found_shown
  guardrails:
    - cast_not_found_shown
    - cast_not_found_go_home
    - cast_not_found_view_docs
decision:
  rule: >-
    This is a measurement (metrics-priority) surface, not an A/B test: the single
    shipping variant instruments the existing StCastNotFound fallback. Read out
    cast_not_found_shown split by { from, reason } to size where the dead-link
    leak comes from, and cast_not_found_recovery_rate = (go_home + view_docs) /
    shown to see how many arrivals recover. Act upstream (link minting / sharing,
    expiry copy) on whichever { from, reason } slice dominates; hold otherwise.
experiment:
  key: st_cast_not_found
  unit: session
  variants:
    - id: instrumented
      weight: 1
      flags:
        instrument: true
  baseline: 0
  mde: 0.05
  min_sample: 2000
---

# Cast: stream link missing / malformed / expired (404 fallback)

The Cast 2.0 "Page Not Found" surface (ui3 `StCastNotFound`, mirroring
decentraland `sites/src/pages/cast/CastNotFoundPage.tsx`) is what a streamer or
watcher lands on when a Cast deep-link has nothing to resolve: `decentraland.org/cast`
with no `:token`/`:location`, a malformed link, or a link whose stream already
ended. Stream links are minted **on-demand** by the Admin Smart Item in a scene
(decentraland/unity-explorer + decentraland/cast), so a bare `/cast` is always a
dead end -- there is no persistent stream directory to fall back to.

This story is a **metrics-priority** surface: a thin route that wraps the existing
`StCastNotFound` 404 and instruments its two recovery CTAs (Go Home / View
Documentation) so we can size the dead-link funnel leak and see where it comes
from (streamer vs watcher) and why (missing / malformed / expired / ended). There
is **no experiment** to run and **no machine** -- the single `instrumented` variant
just turns telemetry on; all copy is verbatim from `StCastNotFound`.

## URL (deep-link addressable)

`/landings/cast-not-found?from=<streamer|watcher|unknown>&reason=<missing|malformed|expired|ended>`

- `from` -- who arrived on the dead link. The real surfaces set this: the streamer
  console (`/cast/s/:token`) routes here as `from=streamer` on an unresolved key;
  the watcher (`/cast/:location`) routes here as `from=watcher`. A bare `/cast`
  with no context is `from=unknown`. Defaults to `unknown`.
- `reason` -- why the link failed: `missing` (no token at all, e.g. bare `/cast`),
  `malformed` (a token that does not parse), `expired` (a key past its TTL), or
  `ended` (the stream the key pointed at has already ended). Defaults to `missing`.

Both params are validated server-side in the loader (anything outside the allowed
sets is coerced to the default) so the readout dimensions stay clean.

## Journey steps

1. `shown` -- render `StCastNotFound` for a missing / malformed / expired / ended
   Cast link; the loader fires `cast_not_found_shown { from, reason }` (the
   dead-link leak counter).
2. `recover` -- the visitor clicks **Go Home** (-> `decentraland.org`) or **View
   Documentation** (-> the Cast docs), each instrumented via a consumer-side
   `onClick` wrapper (no ui3 edit).

## Emitted events

- `experiment_exposed` -- on the fallback rendering (exposure attribution; emitted
  by the loader via `trackExposure`).
- `cast_not_found_shown { from, reason }` -- the 404 fallback rendered. The
  dead-link leak counter and the recovery-rate denominator. `from` 
  `{ streamer, watcher, unknown }`, `reason`  `{ missing, malformed, expired, ended }`.
- `cast_not_found_go_home` -- the **Go Home** CTA was clicked (recovery to
  `decentraland.org`).
- `cast_not_found_view_docs` -- the **View Documentation** CTA was clicked
  (recovery to `docs.decentraland.org/creator/worlds/cast/`).

`cast_not_found_recovery_rate = (cast_not_found_go_home + cast_not_found_view_docs)
/ cast_not_found_shown`.

## Data reality

**No data fetch.** This is a static fallback surface. There is nothing to resolve:
Cast stream links are minted on-demand by the in-scene Admin Smart Item
(decentraland/unity-explorer + decentraland/cast), so a bare `/cast` (and any
stale link) has no backing record on the live catalyst (`catalyst.example.com`). All
copy is verbatim from ui3 `StCastNotFound` (decentraland sites intl
`page.cast.not_found.*` / `page.cast.app.view_docs`). The CTAs are instrumented on
the **consumer** side (a delegated `onClick` wrapper) so telemetry fires without
editing ui3; `track()` is fire-and-forget (sendBeacon survives the Go Home
navigation).
