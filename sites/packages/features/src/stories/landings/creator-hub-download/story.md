---
id: landings-creator-hub-download
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    An OS-detected Creator Hub download page that pre-selects the correct
    installer and offers the desktop tool with a single prominent CTA converts
    more visitors into started downloads than a generic "choose your platform"
    list.
  because: >-
    Auto-detecting the visitor's OS and architecture from the User-Agent removes
    a decision step: the primary CTA already says "Download for macOS/Windows"
    and links straight to the right GitHub release asset, so a larger share of
    page views turn into a download click than when the visitor must first
    identify their own platform.
metric:
  primary: lp_creatorhub_download_ctr
  numerator: lp_creatorhub_download_clicked
  denominator: lp_creatorhub_download_viewed
  guardrails:
    - lp_creatorhub_download_viewed
    - lp_creatorhub_download_clicked
experiment:
  key: lp_creatorhub_download
  unit: session
  variants:
    - id: os_detected
      weight: 1
      flags:
        autoDetectOs: true
  baseline: 0.32
  mde: 0.05
decision:
  rule: >-
    Ship the OS-detected download page if lp_creatorhub_download_ctr
    (lp_creatorhub_download_clicked / lp_creatorhub_download_viewed) clears the
    0.32 baseline with 95% confidence and neither guardrail regresses; otherwise
    iterate on the CTA copy / secondary-OS affordance.
---

# Story -- Download Creator Hub (creator desktop tool)

The Creator Hub download landing (`/landings/creator-hub-download`) is a
server-rendered page that surfaces the **decentraland/creator-hub** desktop
authoring tool. It has no Catalyst crate: download URLs are static links to the
latest GitHub release assets, snapshotted in
`app/fixtures/landings-creator-hub-download.json` (`_source` = the GitHub
Releases API). It composes ui3's `StCreatorHubDownload` and
`StCreatorHubDownloadSuccess`.

## Journey

1. **view** -- `GET /landings/creator-hub-download`. The loader detects the
   visitor OS + architecture from the `User-Agent`, picks the matching installer
   as the primary option, and lists the rest as secondary. Emits
   `lp_creatorhub_download_viewed` (`{ os, arch, version }`).
2. **os-detected** -- the detection result is URL-addressable for QA /
   screenshots: `?os=windows|macos` and `?arch=arm64|amd64` override the
   sniffed value (the CTA re-labels + re-targets the right asset).
3. **download-clicked** -- clicking the primary "Download for &lt;OS&gt;" CTA (or
   an "Also available on" secondary glyph) emits `lp_creatorhub_download_clicked`
   (`{ os, arch, file_name }`) and follows the real GitHub release `<a href>`.
4. **success** -- `?step=success` (optionally `&os=`) renders
   `StCreatorHubDownloadSuccess`, the "You're almost done!" install-steps
   confirmation, and emits `lp_creatorhub_download_success`.

## Metric

Primary `lp_creatorhub_download_ctr = lp_creatorhub_download_clicked /
lp_creatorhub_download_viewed`.

## Notes

The download is a plain link to a GitHub-hosted installer -- there is **no**
backend or on-chain write, so **nothing is simulated**. The flow, states and
metrics are all real; only the live binary fetch happens off-site on GitHub's
CDN. Single-variant story (`os_detected`): this is a "spec" surface (loader +
components), not an A/B wizard, so the experiment block documents the shipped
behavior and exposure attribution rather than splitting traffic.
