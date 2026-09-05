---
id: create-download-hub
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Making "Download Creator Hub" the single, prominent entry on /create routes
    committed creators to the full desktop toolset without cratering the share of
    sessions that reach a preview.
  because: >-
    Some creators want the complete offline toolset (scenes + wearables +
    deploy), and a clear download removes guesswork for them. The risk is an
    install gate before any preview, so this isolates whether a download-first
    entry holds create_preview_rate or sinks it -- the friction floor the other
    entry strategies are measured against.
metric:
  primary: create_preview_rate
  guardrails:
    - create_entry_viewed
    - create_download_clicked
decision:
  rule: >-
    Non-inferiority: ship the download-first entry only if create_preview_rate
    does NOT drop below control by more than the MDE (no significant regression)
    AND create_download_clicked rises; if preview rate regresses past the MDE,
    hold -- the install gate costs more previews than the download is worth.
experiment:
  key: create_download_hub
  unit: session
  variants:
    - id: control
      weight: 1
      flags: {}
    - id: download-hub
      weight: 1
      flags:
        entry: download-hub
  baseline: 0.25
  mde: 0.05
  min_sample: 4000
---

# A download-first /create entry

Standalone test of user story #1: replace the default entry on `/create` with a
single prominent **"Download Creator Hub"** panel and measure the cost on the
shared signal **`/create -> preview`**.

- **Arms:** `control` (today's entry cards) vs `download-hub` (the download panel).
- **Primary:** `create_preview_rate` = sessions with `create_preview` / sessions
  with `experiment_exposed`. Preview after a desktop install rarely happens in the
  same session, so this arm reveals the install-gate friction directly.
- **Guardrails:** `create_entry_viewed` (entry volume holds), `create_download_clicked`
  (download intent -- expected to rise for this arm).
- Rendering + events are shared with the [create-entry-preview] multi-arm test;
  only ONE create experiment runs on `/create` at a time (set via `CREATE_EXPERIMENT`).
