---
id: creator-hub-multiplayer-test
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A first-class Multiplayer Test panel (launch N clients against a scene,
    watch sync live, get a pass/fail sync-health report, replay bugs as
    self-contained bundles) makes creators actually test multiplayer before
    publishing, raising the share of multiplayer scenes that ship without
    production desyncs.
  because: >-
    Today testing multiplayer means hand-launching clients and eyeballing
    state; nobody does it. One button that spawns a measured fleet, an honest
    verdict against creator-set thresholds, and a shareable reproducible bug
    bundle removes the entire setup cost, so sync bugs surface on the
    creator's desk instead of in production join-bursts.
metric:
  primary: ch_mp_run_completed_rate
  guardrails:
    - ch_mp_run_launched
    - ch_mp_run_failed
    - ch_mp_replay_requested
experiment:
  key: ch_mp_scene_testing
  unit: session
  variants:
    - id: panel
      weight: 1
      flags:
        panel: true
  baseline: 0.5
  mde: 0.1
  min_sample: 1000
decision:
  rule: >-
    Ship if launched runs complete at a healthy rate (infra failures are the
    sidecar's fault, not the creator's) and replay gets organic use; hold if
    creators bounce off the launch form or runs routinely die in starting.
---

# Multiplayer scene testing

The Multiplayer Test surface in Creator Hub (`/create/multiplayer-test`):
launch, watch, review, replay. DOM-only; pairs with the local `mp-testd`
sidecar (`tools/mp-testd`, 127.0.0.1:5717) over ws using the
scene-editor-mcp auth model (`?mpd=<port>` + `#mpdtoken=` + localStorage);
zero Tauri IPC; never hosts engine iframes. Contracts per
`rig/docs/mp-scene-testing-design.md` S3.5 and `tools/mp-testd/spec.md`.

1. **unpaired** -- empty state with the copyable sidecar start command
   (`cd tools/mp-testd && npm run build && npm start`). Desktop-shell users on
   an https origin are steered to `http://localhost` (WebKitGTK https->ws
   loopback mixed-content is unverified). Zero console errors with the
   sidecar down.
2. **idle** -- LaunchForm: lane (protocol 2-8 clients; engine/mixed 2-3 -- the
   validated fleet envelope), scene source (project dir / fixture / realm;
   game fixtures protocol-lane-only), scenario (Join burst / Churn / Soak /
   Fuzz), network profile, one-click presets, per-peer + schedule editors,
   pass/fail thresholds. Recent runs list for review.
3. **launching -> running** -- LiveRunView: per-bot table (connected, CRDT
   deltas, server-sync, probe hash), status timeline, log tail, shot strip
   (engine/mixed, polled from the artifact API). Honest "starting" copy:
   first runner boot is 40-100 s.
4. **reviewing** -- ReportView: metric cards (convergence, corrections, join
   burst, never-synced), threshold verdict badge, divergence table (probe
   column labeled "runner store"), perf sparklines, report.html link.
   Lane-honest claim copy per the design S2 matrix.
5. **replay substate** -- ReplayDialog with the two guarantees verbatim:
   **Replay (exact, offline)** (Tier A -- bit-deterministic, shows outcome +
   decision hashes) vs **Reproduce (live, same conditions)** (Tier B --
   outcome-level, honestly non-bit-exact).

- **Primary metric:** `ch_mp_run_completed_rate` =
  `ch_mp_run_completed` / `ch_mp_run_launched`.
- **Events:** `ch_mp_paired`, `ch_mp_run_launched` (`{lane, bots, mode,
  source_kind, profile|preset}`), `ch_mp_run_rejected`, `ch_mp_run_completed`,
  `ch_mp_run_failed`, `ch_mp_replay_requested` (`{tier}`),
  `ch_mp_replay_completed`, `ch_mp_replay_failed`.

## Data reality

The machine (`machine.ts`) is exercised against the real sidecar API shape;
`e2e.mts` drives the full walk (pair -> gate checks -> 2-bot burst ->
starting/running/analyzing/done over ws -> report metrics + divergence table +
verdict -> Tier-A replay outcome hash) via CDP against the worktree dev server
on `127.0.0.1:5197`. Until `tools/mp-testd` lands, `dev-sidecar.mjs` in this
directory implements the S3.5 surface (auth, REST, /events ws, scripted
status walk, report/verdict/replay) so the panel and e2e run against a
spec-faithful stand-in; the e2e passes `--sidecar real` unchanged once the
real daemon is up. Lane caps and fixture gating are enforced in
`@ui/creatorhub/mp/rules` and unit-tested; nothing in the panel fabricates
sync claims -- copy is lane-honest per the design matrix.
