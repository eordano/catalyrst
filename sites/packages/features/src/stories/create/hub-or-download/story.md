---
id: create-hub-or-download
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Offering the Creator Hub in the browser ("Open Creator Hub in your browser")
    alongside the desktop download on /create raises the share of sessions that
    reach a project preview, versus the default entry.
  because: >-
    A web Creator Hub opens a project preview in-page with no install, so more
    sessions reach a preview; the download remains for creators who prefer the
    desktop app.
metric:
  primary: create_preview_rate
  guardrails:
    - create_entry_viewed
    - create_download_clicked
decision:
  rule: >-
    Ship if create_preview_rate beats control by at least the MDE with no
    guardrail regression (entry views hold; download starts don't crater for
    users who genuinely want the desktop app); otherwise hold.
experiment:
  key: create_hub_or_download
  unit: session
  variants:
    - id: control
      weight: 1
      flags: {}
    - id: hub-or-download
      weight: 1
      flags:
        entry: hub-or-download
  baseline: 0.25
  mde: 0.05
  min_sample: 4000
---

# Web Creator Hub or download

Standalone test of user story #3: on `/create`, split the entry into "Open
Creator Hub in your browser" (web hub -> project preview) **or** "Download Creator
Hub", and measure **`/create -> preview`**.

- **Arms:** `control` vs `hub-or-download`.
- **Primary:** `create_preview_rate`; the web-hub CTA emits `create_preview`
  (`path: "web-hub"`), the download CTA emits `create_download_clicked`.
- Rendering + events are shared with the [create-entry-preview] multi-arm test;
  only ONE create experiment runs on `/create` at a time (set via `CREATE_EXPERIMENT`).
