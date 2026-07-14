# sites
DCL sites - a React Router 8 (framework mode, SSR) explorer for Catalyst Places, built as an experimentation platform. Every shippable change is a story: a hypothesis, an experiment, the metrics that decide it, and the surface that implements it. Stories live in `packages/features/src/stories/<id>/`, driven by `story.md` frontmatter.

Visual components come from the sibling `ui3` kit via the `@ui` alias (`@ui/* -> ../ui3/src/*`); do not restructure ui3.

## Packages
Four internal packages under `packages/`, layered strictly downward. Each declares its own
rule in its `package.json` under `sitesBoundary.mayImport`; `scripts/boundaries.test.ts`
enforces it as part of `npm test`.

| Package | Alias | Holds | May import |
|---|---|---|---|
| `@sites/routes` | `@routes/*` | SSR entry points (`root.tsx`, `entry.*`), every route module, route stories | core, data, features |
| `@sites/features` | `@features/*` | per-flow XState machines + wizard components + specs, shared components | core, data |
| `@sites/data` | `@data/*` | catalyst clients, auth, fs, agent, fixtures -- HTTP *and* the direct-Postgres path | core |
| `@sites/core` | `@core/*` | telemetry, experiments, seo, router, content -- leaf utilities | nothing |

`packages/routes/app` is the react-router `appDirectory` (set in `react-router.config.ts`),
so route typegen keeps emitting the `./+types/*` modules the routes import. Cross-package
imports must use the alias; a relative import that escapes its package fails the gate.

## The PE + designer workflow
Five steps from idea to decision; the central artifact is `packages/features/src/stories/<id>/story.md`.
### 1. Define - write the story
```bash
npm run story:new -- <id> --baseline 0.18 --mde 0.02
# multi-step (XState) flow? also scaffold machine.ts / <Comp>.tsx / machine.test.ts:
npm run story:new -- <id> --multi --baseline 0.42 --mde 0.05
```

`story:new` scaffolds `packages/features/src/stories/<id>/` with a valid `story.md` (validated against the shared `StoryMetaSchema` before writing) and computes + writes back `experiment.min_sample`. Fill in the hypothesis, primary metric + guardrails, variants with per-variant `flags`, and `decision.rule`.

Frontmatter shape (single source of truth in `packages/core/src/lib/experiments/context.ts`):

```yaml
id, status, owner
hypothesis: { statement, because }
metric:     { primary, guardrails[] }
experiment: { key, unit, variants:[{ id, weight, flags }], baseline, mde, min_sample }
decision:   { rule }
```

Recompute the sample size when `baseline`/`mde` change:

```bash
npm run sample-size -- --baseline 0.18 --mde 0.02
```
### 2. Build - implement the surface
- Resolve the variant in the route loader with `resolveAssignment(request, story)` (`packages/core/src/lib/experiments/assign.ts`). Always returns a valid `Assignment` (`{ variant, flags, experimentKey }`) even when every backend is down.
- Render the treatment behind `assignment.flags`. Prefer `@ui` atoms/molecules.
- Emit `trackExposure(ctx)` (`packages/core/src/lib/telemetry/track.ts`) only when the experiment surface actually renders; `track(...)` your metric + guardrail events. Both sinks are best-effort and never throw.
- Multi-step flows get an XState `machine.ts` (effects injectable via `input`) plus model-based `machine.test.ts` (pattern: `packages/features/src/stories/jump-in/`).
### 3. Verify
```bash
npm run dev          # SSR dev server with HMR
npm run typecheck    # react-router typegen + tsc --noEmit (strict)
npm run test         # vitest (machine + lib tests)
```

Sanity-check both arms by reading the deterministic local-hash assignment for a given `sid`, or by editing `story.md` variant weights (no kill-switch to force an arm today - see step 4).
### 4. Launch / ramp
`story.md` is the durable definition of the split (`experiment.variants`/`weight`); ramp by editing weights. Assignment is the deterministic local hash of `(sid + experiment.key)`, bucketed by the `story.md` variant weights.

> No instant kill-switch today. `resolveAssignment` checks catalyrst-telemetry `GET /dash/flags` for a per-experiment override, but that endpoint is a global EXPLORER feature-flag boolean map (keys like `explorer-alfa-*`) proxied from the feature-flags service - it has no entry keyed by a sites story `experiment.key`, so the override layer always misses and assignment uses the local hash. A real kill-switch / forced-variant would require a per-experiment override store + endpoint in catalyrst-telemetry (e.g. `GET /dash/experiment/{key}` returning `{killed?, variant?, flags?}`); `getRuntimeFlags` already knows how to consume that shape. To force an arm for QA: read the local-hash assignment for a chosen `sid`, or edit `story.md` variant weights.
### 5. Measure + decide
Exposure + metric events flow to catalyrst-telemetry; readouts (dashboards, funnels) are built in Metabase over the telemetry store. Fixed-horizon verdict via CLI:

```bash
npm run story:readout -- <id>            # human-readable table + verdict
npm run story:readout -- <id> --json     # machine-readable
npm run story:readout -- <id> --alpha 0.01
```

`story:readout` pulls per-variant counts from catalyrst-telemetry (grouped by the `variant` property), computes the primary metric + guardrails per variant, runs a two-proportion z-test (control vs each treatment), checks `min_sample`, and prints SHIP / KILL / KEEP RUNNING against `decision.rule`. With `TELEMETRY_URL` unset it prints a clear message and exits 0. Apply the verdict by editing `story.md` (`status`, weights). There is no telemetry override to flip - see step 4.
## CLI reference
| Command | What it does |
| --- | --- |
| `npm run story:new -- <id> [--multi] [--owner <email>] [--key <flag>] [--baseline <0..1>] [--mde <abs>] [--force]` | Scaffold `packages/features/src/stories/<id>/` with valid `story.md` (+ machine stubs for `--multi`); computes and writes `experiment.min_sample`. |
| `npm run story:readout -- <id> [--alpha 0.05] [--json]` | Read telemetry, compute primary + guardrails per variant, two-proportion z-test, verdict vs `decision.rule`. |
| `npm run sample-size -- --baseline <0..1> --mde <abs> [--alpha 0.05] [--power 0.8]` | Per-variant minimum sample size for a two-proportion test. |
| `npm run dev` / `npm run build` / `npm run start` | React Router dev / production build / serve. |
| `npm run typecheck` / `npm run test` | `tsc --noEmit` (strict) / vitest. |
## Configuration (env vars)
All vars are optional; the app degrades gracefully and never throws if any are unset or unreachable. See `.env.example`.

| Var | Used by | When unset |
| --- | --- | --- |
| `CATALYST_URL` | Catalyst Places fetches (`packages/data/src/lib/catalyst/*`) | Defaults to `https://catalyst.example.com`. |
| `TELEMETRY_URL` | catalyrst-telemetry sink (`track` -> `/v1/track`), the runtime-flag probe (`resolveAssignment` -> `/dash/flags`, currently a no-op override layer), and `story:readout`. | Sink + flag probe become safe no-ops; `story:readout` prints a clear message and exits 0. |
## Flag-eval + telemetry - roles
One backend: catalyrst-telemetry - the authoritative event store and the source the readouts query. Flag evaluation has two layers, but the override layer is currently inert (see step 4 note).

| Concern | catalyrst-telemetry (`TELEMETRY_URL`) |
| --- | --- |
| Event capture | `POST /v1/track` (Segment `track` shape), `Authorization: Basic dcl-sites` |
| Assignment precedence | (1) highest - per-experiment override from `GET /dash/flags` (today: never matches) |
| Instant kill-switch / ramp without redeploy | no - not implemented in catalyrst-telemetry |
| Experiment definition / variants | `story.md` (variant weights drive the local hash) |
| Readout source | `story:readout` CLI (`/dash/sql` filtered by `exp_key` + grouped by `variant`/`event`); `/dash/breakdown` / `/dash/events` as JSON drill-downs |
| Identity | `anonymousId` = `sid` cookie |
| When unset/unreachable | sink + flag probe are no-ops; assignment falls back to local hash |

`GET /dash/flags` returns `{config:{flags:{"explorer-...":bool},variants:{...}}}` - no key for a sites `experiment.key`, so (2) the local hash (formula in step 4) always decides; the app never breaks.
