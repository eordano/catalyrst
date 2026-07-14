---
id: create-entry-preview
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    On /create, giving visitors a low-friction, in-browser path to a creation
    PREVIEW -- a builder-style wearable flow, a web Creator Hub, or a
    capability-routed web/download choice -- raises the share of sessions that
    reach a preview, versus pushing them to download the desktop Creator Hub
    first.
  because: >-
    Downloading and installing a desktop app is the highest-friction first step;
    most visitors evaluating "can I create here?" never return after a download
    prompt. An in-browser path that lands them in a live preview (avatar / scene)
    removes the install gate, so more sessions reach the aha-moment of seeing
    their creation. Modern Chromium browsers expose the File System Access API,
    so capable users can open the web hub with real local folder/file access
    while everyone else still gets the download.
metric:
  primary: create_preview_rate
  guardrails:
    - create_entry_viewed
    - create_download_clicked
decision:
  rule: >-
    Proper multi-arm: with four treatment arms vs one control, judge each arm
    against control at the Bonferroni-adjusted significance alpha/4 = 0.0125 (not
    0.05) to hold the family-wise false-positive rate. Ship the arm with the
    highest create_preview_rate that clears the adjusted bar by at least the MDE
    with no guardrail regression (entry views hold; download starts don't crater
    for users who genuinely want the desktop app). If two or more arms qualify,
    ship the best and re-confirm it against control in a follow-up two-arm holdout
    before full ramp; if none clear the adjusted bar at min_sample per arm, hold.
    Downstream editors/preview are existing surfaces and the in-editor commit stays
    simulated, so the readout judges /create -> preview intent, not the save.
experiment:
  key: create_entry_preview
  unit: session
  variants:
    - id: control
      weight: 1
      flags: {}
    - id: download-hub
      weight: 1
      flags:
        entry: download-hub
    - id: builder-or-download
      weight: 1
      flags:
        entry: builder-or-download
    - id: hub-or-download
      weight: 1
      flags:
        entry: hub-or-download
    - id: capability-routed
      weight: 1
      flags:
        entry: capability-routed
        webHubIfCapable: true
  baseline: 0.25
  mde: 0.05
  min_sample: 4000
---

# Get a /create visitor to a preview

`/create` is the front door for creators. The fastest proof that "I can make
something here" is reaching a **preview** -- a live avatar in the wearable
item-editor, or a project open in the (web) Creator Hub. The cost of that proof
today depends entirely on which entry we put in front of the visitor.

This experiment compares five entry strategies on the single signal
**`/create -> preview`**, randomized per session and bucketed deterministically
by `sid` (assign.ts). Each arm renders a different entry surface above the hub;
the conversion is the same everywhere.

## Arms

- **control** -- today's entry-card row. The Make-a-Wearable card already deep-links
  into the item-editor, which is an in-browser preview, so control has a real
  baseline `create_preview_rate`.
- **download-hub** (#1) -- a single prominent "Download Creator Hub" panel. The
  desktop app is the only offered path; preview happens after install (rarely in
  the same session) -- the friction floor we measure the others against.
- **builder-or-download** (#2) -- a split: "Make a wearable in your browser"
  (builder-style, no install -> item-editor preview) **or** "Download Creator
  Hub".
- **hub-or-download** (#3) -- a split: "Open Creator Hub in your browser"
  (web hub -> project preview) **or** "Download Creator Hub".
- **capability-routed** (#4) -- detects the **File System Access API**
  (`showDirectoryPicker`, i.e. a modern Chromium-based browser). If present, the
  primary CTA opens the **web** Creator Hub with real local folder/file access;
  otherwise it falls back to the download. Same single CTA, capability-gated.

`?arm=<variant>` forces an arm (preview/QA only) so every entry surface is
URL-addressable and screenshottable.

## Events

| event | when | role |
| --- | --- | --- |
| `experiment_exposed` | /create renders the assigned arm | exposure (denominator) |
| `create_entry_viewed` | the entry surface renders | guardrail (entry volume holds) |
| `create_preview` | session reaches an in-browser preview (item-editor / web hub) | **primary numerator** |
| `create_download_clicked` | session chooses the desktop download | guardrail |
| `create_capability_detected` | capability-routed arm only | diagnostic: `{ fs_access, route }` |

- **Primary metric:** `create_preview_rate` = DISTINCT sessions with `create_preview`
  / DISTINCT sessions with `experiment_exposed`, scoped to one variant.
- `create_preview` carries `{ path: "builder" | "web-hub", target }`; the download
  CTAs carry `{ from: <arm> }`. The teleport into the editor is real navigation;
  the in-editor save/commit stays simulated downstream.
