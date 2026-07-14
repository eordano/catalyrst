---
id: create-capability-routed
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Routing capable browsers (File System Access API) straight to the web Creator
    Hub -- and everyone else to the download -- raises the share of /create
    sessions that reach a preview, versus the default entry.
  because: >-
    Modern Chromium browsers expose showDirectoryPicker, so capable users can run
    the web hub with real local folder/file access and reach a preview with zero
    install, while users on browsers without the API still get a working download
    path. Capability-gating offers each visitor the lowest-friction route they
    can actually use.
metric:
  primary: create_preview_rate
  guardrails:
    - create_entry_viewed
    - create_download_clicked
decision:
  rule: >-
    Ship if create_preview_rate beats control by at least the MDE with no
    guardrail regression; segment the readout by create_capability_detected so the
    lift is attributable to the FS-Access-capable cohort and the non-capable cohort
    is confirmed no worse than control. Otherwise hold.
experiment:
  key: create_capability_routed
  unit: session
  variants:
    - id: control
      weight: 1
      flags: {}
    - id: capability-routed
      weight: 1
      flags:
        entry: capability-routed
        webHubIfCapable: true
  baseline: 0.25
  mde: 0.05
  min_sample: 4000
---

# Capability-routed: web hub or download

Standalone test of user story #4: on `/create`, detect the **File System Access
API** (`showDirectoryPicker`, a modern Chromium-based browser). If present, the
primary CTA opens the **web** Creator Hub with real local folder/file access;
otherwise it falls back to the **download**. One CTA, capability-gated.

- **Arms:** `control` vs `capability-routed`.
- **Primary:** `create_preview_rate`. The capability arm emits
  `create_capability_detected { fs_access, route }` once per session, then either
  `create_preview` (`path: "web-hub"`, capable) or `create_download_clicked`
  (not capable) on the CTA.
- SSR-safe progressive enhancement: with no JS the download fallback renders; the
  web-hub CTA upgrades on the client when the API is present.
- Rendering + events are shared with the [create-entry-preview] multi-arm test;
  only ONE create experiment runs on `/create` at a time (set via `CREATE_EXPERIMENT`).
