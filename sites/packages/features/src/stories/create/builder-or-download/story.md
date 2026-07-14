---
id: create-builder-or-download
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Offering an in-browser builder ("Make a wearable in your browser") alongside
    the desktop download on /create raises the share of sessions that reach a
    preview, versus the default entry.
  because: >-
    The browser path lands creators directly in the item editor's live avatar
    preview with no install, so more sessions hit the aha-moment of seeing their
    creation; the download stays for creators who want the full desktop app.
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
  key: create_builder_or_download
  unit: session
  variants:
    - id: control
      weight: 1
      flags: {}
    - id: builder-or-download
      weight: 1
      flags:
        entry: builder-or-download
  baseline: 0.25
  mde: 0.05
  min_sample: 4000
---

# Builder-in-browser or download

Standalone test of user story #2: on `/create`, split the entry into "Make a
wearable in your browser" (Builder -> item-editor preview, no install) **or**
"Download Creator Hub", and measure **`/create -> preview`**.

- **Arms:** `control` vs `builder-or-download`.
- **Primary:** `create_preview_rate`; the builder CTA emits `create_preview`
  (`path: "builder"`), the download CTA emits `create_download_clicked`.
- Rendering + events are shared with the [create-entry-preview] multi-arm test;
  only ONE create experiment runs on `/create` at a time (set via `CREATE_EXPERIMENT`).
