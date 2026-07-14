---
id: governance-projects
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A filterable Projects list (category / grant subtype / status / quarter as
    URL params) with per-quarter funding stats helps visitors find a relevant
    funded grant or bid project and open its detail faster.
  because: >-
    The DAO funds hundreds of grant + bidding/tendering projects across many
    categories and quarters; URL-addressable filters plus quarter funding stats
    let a visitor narrow to what they care about, raising the share of list views
    that end in opening a project detail.
metric:
  primary: gv_project_clicked
  guardrails:
    - gv_projects_viewed
    - gv_projects_filtered
experiment:
  key: gv_projects_browse
  unit: session
  variants:
    - id: filterable-list
      weight: 1
      flags:
        urlFilters: true
        quarterStats: true
  baseline: 0.28
  mde: 0.03
  min_sample: 5000
decision:
  rule: >-
    Ship if the detail-open rate (gv_project_clicked / gv_projects_viewed)
    improves by at least the MDE with no guardrail regression in
    gv_projects_viewed volume or gv_projects_filtered usage; otherwise hold.
---

# Governance projects -- browse Grants & Bidding/Tendering

The Projects list (`/governance/projects`) is a loader + components surface (no
machine -- it is a browse/detail flow, not a multi-step wizard). The loader reads
`?category` (grants | bidding) / `?subtype` (grant subtype) / `?status`
(ongoing | finished | paused) / `?year` + `?quarter` (Q1-Q4) / `?sort` from the
URL, fetches the live DAO governance projects API best-effort, filters + sorts on
the server, computes quarter/aggregate funding stats, and returns plain data so
the page works with JS disabled. The component composes ui3's `GvProjectsList`
(via the instrumented `ProjectsList` wrapper) inside `GovernanceChrome`.

Governance is NOT a Catalyst service. The DAO runs on Snapshot + Aragon with its
own API at `https://governance.decentraland.vote/api/projects`
(decentraland/governance `src/clients/Governance.ts` -> `GOVERNANCE_API`). We try
that live endpoint and fall back to the local fixture
(`app/fixtures/governance-projects.json`, faithful `ProjectInList` derivations)
on any failure / empty response. Detail links route to
`/governance/projects/:id` (the detail page is deferred/out of scope here -- the
list is the shipping surface).

- **Primary metric:** `gv_project_clicked` -- detail-open rate from the list.
- **Guardrails:** `gv_projects_viewed` volume and `gv_projects_filtered` usage.
- **Events:**
  - `gv_projects_viewed` -- on load / when the filtered result set changes
    (carries count, total, category, subtype, status, year, quarter, sort,
    source live|fixture).
  - `gv_projects_filtered` -- on a filter/sort change (carries the active filter
    state) so we can attribute drop-off to a specific facet.
  - `gv_project_clicked` -- on a ProjectCard click ({ project_id, type, category,
    status }) before navigating to `/governance/projects/:id`.

Single shipping variant (`filterable-list`); the experiment schema is kept valid
for the readout tooling and deterministic bucketing even though all traffic sees
the same surface.

## Journey (URL-addressable)

| Step | URL |
| --- | --- |
| Projects list (default) | `/governance/projects` |
| Category filter (grants) | `/governance/projects?category=grants` |
| Grant subtype filter | `/governance/projects?category=grants&subtype=platform` |
| Bidding/tendering | `/governance/projects?category=bidding` |
| Status filter | `/governance/projects?status=ongoing` |
| Quarter filter | `/governance/projects?year=2026&quarter=Q1` |
| Sort | `/governance/projects?sort=size` |
| Open a project | `/governance/projects/:id` (deferred detail) |
