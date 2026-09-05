---
id: creator-curate-committee
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Letting a committee curator attach an approve/reject COMMENT inline with the
    decision (review -> compose comment -> decide) increases the share of
    decisions that ship with a written rationale, without slowing the queue --
    even with the on-chain item-curation PATCH and the forum comment post
    simulated.
  because: >-
    A required-feeling but optional comment box surfaced at the moment of
    decision lowers the friction of explaining a rejection (today that reasoning
    lives in a separate forum tab), so curators leave a rationale far more often,
    which gives creators actionable feedback instead of an opaque rejection.
metric:
  primary: bd_curation_comment_rate
  numerator: bd_curation_comment_added
  denominator: bd_curation_decided
  guardrails:
    - bd_curation_decided
    - bd_curation_review_opened
experiment:
  key: bd_curation_comments
  unit: session
  variants:
    - id: comments
      weight: 1
      flags:
        comments: true
        commentRequiredOnReject: false
  baseline: 0.45
  mde: 0.05
  min_sample: 2000
decision:
  rule: >-
    Ship if bd_curation_comment_rate (decisions with bd_curation_comment_added /
    decisions with bd_curation_decided) improves by at least the MDE with no
    guardrail regression (decision volume and review-open volume hold);
    otherwise hold.
---

# Committee curation dashboard with approve/reject comments (committee-gated)

The committee curation dashboard (`/create/curate`) lists every submitted
collection with its curation status, type, owner, assignee and forum discussion
-- and, NEW for this story, the prior committee **comment thread** on each
collection. A committee member filters the queue (`?status=to_review`), assigns
a collection to themselves inline on the dashboard, opens its items for review
(`?step=review&id=...`), drafts a decision and composes a comment
(`?step=comment`), then commits the approve/reject + comment (`?step=decided`).
The transient assign/decide spinners are never written to the URL, and deep
links to `?step=assign` / `?step=decide` land on the nearest stable step
(dashboard / comment) instead of hydrating a spinner whose request never fires.
This story tracks whether the inline comment composer increases the share of
decisions that ship with a written rationale.

- **Primary metric:** `bd_curation_comment_rate` = `bd_curation_comment_added`
  / `bd_curation_decided` (per decision).
- **Guardrails:** decision volume (`bd_curation_decided`) and review-open volume
  (`bd_curation_review_opened`) must stay healthy.
- **Events:** `bd_curation_decided` (`{id, status}`, stub) on commit,
  `bd_curation_comment_added` (`{id, decision, length}`, stub) when a comment is
  posted alongside the decision, and `bd_curation_viewed` on render.

Data reality: the dashboard rows are read LIVE from the admin-gated
catalyrst-builder `GET /v1/collections/curation` (the server-only loader
authenticates with `CATALYRST_BUILDER_ADMIN_TOKEN`; without it the request 403s
and the route degrades to an honest error banner -- **no fixture rows are
rendered**; `app/fixtures/creator-curate-committee.json` remains only as a
shape witness for tests). Rows follow builder-server's `CollectionCuration`
shape (status `pending|approved|rejected`, nullable `assignee`), with the
dashboard display state derived exactly as the builder frontend's
`getCollectionCurationState()`. There is no native comment column on
`CollectionCuration` upstream -- the curator's reasoning is posted to the
collection's forum topic as a `ForumNewPost { raw, topic_id }`
(builder-server `src/Forum/Forum.types.ts`), so `CurationComment` is a faithful
projection of that flow; the live queue endpoint carries no comment history, so
the review step's prior-comments thread is empty until a comments read-path
exists. The live catalyrst-builder PATCH
`/v1/collections/{id}/items/{item}/status` plus the forum post are bearer/
committee gated, so the approve/reject + comment **commit is SIMULATED** and
the UI says so ("This is a preview and will not be posted yet") -- the XState
wizard's request bodies mirror those handlers (`{ status }` and
`{ raw, topic_id }`). The assign-to-me write mirrors
`/collections/{id}/curation { assignee }`. The wizard itself is gated to
connected committee wallets; a non-committee viewer (or `?committee=false`)
gets the graceful shield EmptyState, and a committee viewer with zero rows the
"no collections to review yet." state.
