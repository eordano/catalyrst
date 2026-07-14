---
id: governance-profile-activity
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Surfacing a wallet's full governance activity feed -- votes cast, proposals
    authored, VP distribution, and delegation graph -- on a single profile page
    increases cross-session re-engagement and VP delegation initiated from the
    profile.
  because: >-
    Users who can see their full DAO footprint (proposals, watchlist,
    co-authoring, voted proposals, VP breakdown) on one page have a clear mental
    model of their influence, which drives them to expand it via delegation or
    new proposal authorship; currently the data is scattered across multiple
    screens, leading to drop-off before a delegation or authorship action occurs.
metric:
  primary: gv_profile_viewed
  guardrails:
    - gv_profile_tab_changed
    - gv_profile_proposal_clicked
    - gv_profile_delegate_clicked
experiment:
  key: gv_profile_activity
  unit: session
  variants:
    - id: full-feed
      weight: 1
      flags:
        showVotedProposals: true
        showDelegation: true
        showProjects: true
  baseline: 0.2
  mde: 0.04
  min_sample: 5000
decision:
  rule: >-
    Ship if gv_profile_viewed sessions convert to gv_profile_delegate_clicked or
    gv_profile_proposal_clicked at or above the MDE over baseline with no
    guardrail regression; otherwise hold.
---

# Governance profile activity -- wallet DAO dashboard

`/governance/profile/activity` (optionally `?address=0x...` to view another
wallet's profile, `?tab=proposals|watchlist|coauthoring` to deep-link into the
activity tab).

The profile page is the single-pane DAO dashboard for a wallet: votes cast,
proposals authored/co-authored/subscribed, VP breakdown by asset, delegation,
and voting stats. ui3's `GvProfileActivity` renders the full composition inside
`GovernanceChrome`.

Governance is NOT a Catalyst service (Snapshot + Aragon externally). VP values,
voting stats, delegation graph, and voted proposals are SIMULATED/FIXTURE. The
authored-proposals tab attempts a best-effort live fetch from
`GET governance.decentraland.vote/api/proposals?user=<addr>`; on failure (the
sandbox has no egress to that host) it falls back silently to fixture rows so
SSR always succeeds.

## Events

- `gv_profile_viewed {address, tab, proposals_count}` -- on mount.
- `gv_profile_tab_changed {from_tab, to_tab}` -- when the activity tab changes.
- `gv_profile_proposal_clicked {proposal_id, tab}` -- row click in the activity
  box.
- `gv_profile_delegate_clicked {}` -- "Change Delegation" action click.

## Journey steps

1. `/governance/profile/activity` -- own profile (fixture address).
2. `/governance/profile/activity?tab=watchlist` -- watchlist tab.
3. `/governance/profile/activity?tab=coauthoring` -- co-authoring tab.
4. `/governance/profile/activity?address=0x000000000000000000000000000000000000dead` --
   another wallet's read-only view.
