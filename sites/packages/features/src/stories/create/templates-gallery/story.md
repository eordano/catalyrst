---
id: create-templates-gallery
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A browsable templates gallery as the new-scene entry point increases the
    rate of scene-list / empty-state visitors who select a starting template.
  because: >-
    A browsable gallery lowers the cost of choosing a starting point, so more
    creators pick a template and continue into the studio instead of
    abandoning at the blank-scene decision.
metric:
  primary: ch_template_selected_rate
  guardrails:
    - ch_templates_viewed
experiment:
  key: ch_templates_gallery
  unit: session
  variants:
    - id: gallery
      weight: 1
      flags:
        gallery: true
  baseline: 0.3
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if ch_template_selected_rate improves by at least the MDE with no
    guardrail regression (gallery view volume holds); otherwise hold.
---

# Browse templates to start a new scene

The Templates gallery (`/create/templates`) is the new-scene entry point. This
story tracks whether a browsable gallery increases the rate of visitors who
select a starting template and continue into the studio.

- **Primary metric:** `ch_template_selected_rate` = `ch_template_selected` / `ch_templates_viewed`.
- **Guardrails:** gallery view volume (`ch_templates_viewed`) must hold.
- **Events:** `ch_templates_viewed` on load, `ch_template_selected` on a card
  (`{template_id}`), `ch_studio_opened` (`{source:'template'}`) on continue.

> **No filters / no sort (decision, deep review):** the gallery
> deliberately ships WITHOUT filter chips, sort controls or a result count --
> the filter row that once existed was removed on explicit owner direction
> ("Don't add filters/sort to these templates"). The template set is small
> enough to scan whole. Do not re-add `ch_template_filtered` or a
> `?difficulty=` param; this section supersedes the earlier "filterable,
> difficulty-filtered" framing of this story.

Live data: templates are NOT served by Catalyst (builder routes 404 publicly);
the gallery renders the ui3 component's built-in templates (render-only). Noted
as deferred.
