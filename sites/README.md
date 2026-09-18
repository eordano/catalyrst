# sites
DCL sites - a React Router 8 (framework mode, SSR) explorer for Catalyst Places, built as an experimentation platform: every shippable change is a story (hypothesis, experiment, deciding metrics, surface). Stories live in `packages/features/src/stories/<id>/`, driven by `story.md` frontmatter.

Visual components come from the sibling `ui3` kit via the `@ui` alias (`@ui/* -> ../ui3/src/*`); do not restructure ui3.

## Packages
Four internal packages under `packages/`, layered strictly downward. Each declares its own
rule in its `package.json` under `sitesBoundary.mayImport`; `scripts/boundaries.test.ts`
enforces it as part of `npm test`.

| Package | Alias | Holds | May import |
| --- | --- | --- | --- |
| `@sites/routes` | `@routes/*` | SSR entry points (`root.tsx`, `entry.*`), every route module, route stories | core, data, features |
| `@sites/features` | `@features/*` | per-flow XState machines + wizard components + specs, shared components | core, data |
| `@sites/data` | `@data/*` | catalyst clients, auth, fs, agent, fixtures -- HTTP *and* the direct-Postgres path | core |
| `@sites/core` | `@core/*` | telemetry, experiments, seo, router, content -- leaf utilities | nothing |

`packages/routes/app` is the react-router `appDirectory` (set in `react-router.config.ts`),
so route typegen keeps emitting the `./+types/*` modules the routes import. Cross-package
imports must use the alias; a relative import that escapes its package fails the gate.

## The PE + designer workflow
Five steps from idea to decision around `packages/features/src/stories/<id>/story.md`.
### 1. Define - write the story
```bash
npm run story:new -- <id> --baseline 0.18 --mde 0.02
# multi-step (XState) flow? also scaffold machine.ts / <Comp>.tsx / machine.test.ts:
npm run story:new -- <id> --multi --baseline 0.42 --mde 0.05
```

`story:new` scaffolds the story dir with a `story.md` validated against the shared `StoryMetaSchema` before writing, and computes + writes back `experiment.min_sample`. Fill in the hypothesis, primary metric + guardrails, variants with per-variant `flags`, and `decision.rule`.

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
npm run test         # vitest (all surviving contract tests)
```

The unit run executes every surviving test. Presentation-only render checks are
not tests: keep those examples as stories, and add a test only for a durable
security, data-integrity, accessibility, cross-runtime, or user-visible contract.

To sanity-check assignment, use a fixed `sid`. To force an arm, use the runtime overrides described below or edit `story.md` variant weights.
### 4. Launch / ramp
`story.md` is the durable definition of the split (`experiment.variants`/`weight`); ramp by editing weights. Assignment is the deterministic local hash of `(sid + experiment.key)`, bucketed by the `story.md` variant weights.

Runtime controls are implemented. `resolveAssignment` first applies explicit dashboard flag overrides from `GET /dash/flags`, then per-experiment controls from `GET /dash/experiments?key=<experiment.key>`, then the local hash. Experiment controls accept `killed`, `variant` and `flags`; `killed` pins assignment to the selected or default variant rather than removing the surface. Runtime responses are cached for 15 seconds. If the probe fails, assignment falls back to the local definition.
### 5. Measure + decide
Exposure + metric events flow to catalyrst-telemetry; readouts (dashboards, funnels) are built in Metabase over the telemetry store. Fixed-horizon verdict via CLI:

```bash
npm run story:readout -- <id>            # human-readable table + verdict
npm run story:readout -- <id> --json     # machine-readable
npm run story:readout -- <id> --alpha 0.01
```

`story:readout` pulls per-variant counts from catalyrst-telemetry (grouped by the `variant` property), computes the primary metric + guardrails per variant, runs a two-proportion z-test (control vs each treatment), checks `min_sample`, and prints SHIP / KILL / KEEP RUNNING against `decision.rule`. With `TELEMETRY_URL` unset it prints a clear message and exits 0. Apply the verdict deliberately through runtime overrides or by editing `story.md` (`status`, weights); the readout does not apply it automatically.
## CLI reference
| Command | What it does |
| --- | --- |
| `npm run story:new -- <id> [--multi] [--owner <email>] [--key <flag>] [--baseline <0..1>] [--mde <abs>] [--force]` | Scaffold `packages/features/src/stories/<id>/` with valid `story.md` (+ machine stubs for `--multi`); computes and writes `experiment.min_sample`. |
| `npm run story:readout -- <id> [--alpha 0.05] [--json]` | Read telemetry, compute primary + guardrails per variant, two-proportion z-test, verdict vs `decision.rule`. |
| `npm run sample-size -- --baseline <0..1> --mde <abs> [--alpha 0.05] [--power 0.8]` | Per-variant minimum sample size for a two-proportion test. |
| `npm run dev` / `npm run build` / `npm run start` | React Router dev / production build / serve. |
| `npm run typecheck` / `npm run test` | `tsc --noEmit` (strict) / vitest. |
## Configuration (env vars)
Defaults and failure behavior vary by capability. Assignment and telemetry tolerate unavailable services; writes must report missing authentication or unavailable backends without claiming success. See `.env.example`.

| Var | Used by | When unset |
| --- | --- | --- |
| `CATALYST_URL` | Catalyst Places fetches (`packages/data/src/lib/catalyst/*`) | Defaults to `https://catalyst.example.com`. |
| `TELEMETRY_URL` | catalyrst-telemetry sink (`track` -> `/v1/track`), runtime controls (`resolveAssignment` -> `/dash/flags` and `/dash/experiments`), and `story:readout`. | Sink + flag probe become safe no-ops; `story:readout` prints a clear message and exits 0. |
## Flag-eval + telemetry - roles
One backend: catalyrst-telemetry - the authoritative event store and the source the readouts query. Flag evaluation combines runtime overrides with the local story definition (see step 4).

| Concern | catalyrst-telemetry (`TELEMETRY_URL`) |
| --- | --- |
| Event capture | `POST /v1/track` (Segment `track` shape), `Authorization: Basic dcl-sites` |
| Assignment precedence | Explicit dashboard flag override, then per-experiment override, then local hash |
| Runtime control without redeploy | Pin a variant or override flags; `killed` pins the selected/default variant. Cached for 15 seconds. |
| Experiment definition / variants | `story.md` (variant weights drive the local hash) |
| Readout source | `story:readout` CLI (`/dash/sql` filtered by `exp_key` + grouped by `variant`/`event`); `/dash/breakdown` / `/dash/events` as JSON drill-downs |
| Identity | `anonymousId` = `sid` cookie |
| When unset/unreachable | sink + flag probe are no-ops; assignment falls back to local hash |


## Product capabilities and assumptions

The product review applies the same five questions
(identity, availability, persistence, completion and measurement) to every story
family. It distinguishes implemented paths, partial integrations, previews and
unavailable actions, with source evidence. It is a code audit, not a live uptime
or end-to-end certification.

```bash
npm run products:audit          # audit product coverage and evidence paths
npm run products:audit -- --write # refresh the readable report
npm run products:check          # reject missing groups or a stale report
```

Edit `docs/product-capabilities.json` when capabilities or prerequisites change.
A story hypothesis is not a verified capability. Keep a one-arm flow separate
from comparative experiments, and do not fill missing owners or measured
baselines with invented values. Demo steps and signatures are not evidence of
persisted writes or completed transactions.
