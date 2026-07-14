---
id: legal
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A single config-driven legal route covering all policy docs lets visitors
    find and read terms/privacy/content/ethics with low bounce.
  because: >-
    One consistent layout with a shared sidebar + table of contents makes every
    policy document navigable the same way, so visitors locate the section they
    need and engage (scroll/click anchors) rather than bouncing.
metric:
  primary: lp_legal_viewed
  guardrails:
    - lp_legal_section_clicked
experiment:
  key: lp_legal_docs
  unit: session
  variants:
    - id: config_driven
      weight: 1
      flags:
        sharedLayout: true
  baseline: 0.4
  mde: 0.05
decision:
  rule: >-
    Keep the single config-driven legal route if lp_legal_viewed holds across all
    eight slugs and lp_legal_section_clicked (TOC anchor depth) shows engagement;
    otherwise revisit the layout.
---

# Story -- Read a legal document by slug

`/legal/:doc` resolves `:doc` against the ui3 `LEGAL_DOCS` map
(`terms | privacy | content | ethics | rewards | referral | security | brand`)
and renders the shared `LegalDocPageLayout` (sidebar + TOC + sections). An
unknown slug returns 404 with a graceful in-page empty state.

- `lp_legal_viewed` fires on load (`{ doc }`).
- Following a TOC / sidebar anchor emits `lp_legal_section_clicked`
  (`{ doc, section }`).
