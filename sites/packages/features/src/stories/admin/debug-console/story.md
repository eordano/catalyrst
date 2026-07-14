---
id: admin-debug-console
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Giving the DAO ops/committee an in-product read-only governance debug & ops
    console (app version, service health, frontend env vars, Snapshot config and
    a transparency-budgets summary) surfaced inside GvDebugAdmin lets authorized
    admins diagnose governance issues faster than jumping between the API, the
    Snapshot space and the env config.
  because: >-
    Today the governance maintenance tools (TriggerFunction, BadgesAdmin,
    InvalidateCache, Notifications, Snapshot, QueryData) live behind a
    debug-address gate with no consolidated read view of the things an operator
    checks first -- what version is live, is the service healthy, what env is it
    pointed at, and what are the current transparency budgets. A single gated
    console that renders those reads up front (with the write tools clearly
    marked as committee-gated) makes the first diagnostic pass a single glance.
metric:
  primary: admin_debug_console_viewed
  guardrails:
    - admin_debug_panel_switched
experiment:
  key: admin_debug_console
  unit: session
  variants:
    - id: console
      weight: 1
      flags:
        console: true
  baseline: 0.05
  mde: 0.02
  min_sample: 2000
decision:
  rule: >-
    Ship if admin_debug_console_viewed is non-zero and admins switch panels
    (admin_debug_panel_switched) at least once per viewed session on average;
    otherwise hold.
---

# Admin -- Governance debug & ops console (GvDebugAdmin maintenance tools)

A gated, READ-ONLY governance debug & ops console at `/admin/debug-console` for
the ADMIN persona. It renders ui3's `GvDebugAdmin` (inside its own
`GovernanceChrome`) bound to live read data, mirroring the Decentraland
governance internal debug page (`decentraland/governance` src/routes/debug.ts +
the legacy `src/pages/debug.tsx` Admin/Debug panels).

## Data

- **Live reads (best-effort, via `catalyrst-governance`):**
  - `GET /governance/health` -- service liveness.
  - `GET /governance/budgets?limit=N` -- archived transparency-budget quarters
    (`{ start, finish, total, category_percentages }`), the same data the Admin
    panel's *Fetch Transparency Budgets* button returns.
- **Fixture-only reads:** app version, the frontend env-var snapshot the
  `EnvStatus` tool inspects (`GATSBY_GOVERNANCE_API`, `GATSBY_SNAPSHOT_API`,
  `GATSBY_SNAPSHOT_SPACE`, `GATSBY_DEFAULT_CHAIN_ID`, ...), the Snapshot space
  config, and the `TriggerFunction` function names (`runQueuedAirdropJobs`,
  `giveAndRevokeLandOwnerBadges`, `giveTopVoterBadges`,
  `restoreMissingUpdatesForumPost`).

Both `/governance/*` paths 404 on `https://catalyst.example.com` today (the crate is
staged but not yet deployed), so the loader degrades to
`app/fixtures/admin-debug-console.json`. The fetcher never throws.

## There is no gate, because there is nothing to gate

Both reads behind this page are public. Neither
`catalyrst-governance/src/handlers/health.rs:3` nor
`catalyrst-governance/src/handlers/read.rs:220` takes an auth extractor of any
kind, so an anonymous caller is entitled to exactly this page. It renders
unconditionally and labels itself "public data -- no authorization required".

`?authorized=1` has been removed. It was never an access control: it was a query
parameter the visitor sets, checked by nobody, that turned a 17,640-byte SSR
response into a 27,891-byte one for the same anonymous caller. It is removed
rather than hidden -- no code reads it, so there is no state it can still reach.

## Tool actions -- removed, not simulated

`BudgetsUpdate`, `BadgesAdmin`, `TriggerFunction`, `Notifications`,
`InvalidateCache`, `ErrorReporting`, `HttpStatus`, `EnvStatus`, `Snapshot` and
`QueryData` are gone from this route, along with the `GvDebugAdmin` form wall
they lived in.

Not one of them ever made an HTTP call -- there is no endpoint behind any of
them, on this node or upstream -- while presenting as privileged maintenance
operations and emitting `admin_debug_tool_invoked`. They are replaced by one
permanent unavailable notice carrying that fact
(`control-availability.ts` -> `debug.tools`). `admin_debug_tool_invoked` has
been dropped from the telemetry catalog.

`GvDebugAdmin` itself is untouched in ui3, where it remains a Storybook
component.

## Data

- **Live public reads** (`catalyrst-governance`):
  - `GET /governance/health` -- service liveness.
  - `GET /governance/budgets?limit=N` -- archived transparency-budget quarters.
- **Page chrome, not measurements:** app version, the env-var *names* the page
  documents, and the Snapshot space identifiers. These are descriptive labels.

A failed read is reported, never replaced. `health` and `budgets` are `null`
with a reason string. The previous version fell back to
`src/fixtures/admin-debug-console.json` on any failure, so a node whose
governance service was down rendered a healthy status pill and a full budget
table.

## Journey + metrics

- `/admin/debug-console` -- `admin_debug_console_viewed` `{ panel }` on mount.
- Switch the Admin/Debug tabs -- `admin_debug_panel_switched` `{ panel }`.
