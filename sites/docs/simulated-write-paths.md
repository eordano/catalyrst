# Simulated write-paths - real-backend checklist
Reads are real across the site; the write / action paths below are disclosed simulations - each returns `{ simulated: true }` / `{ stub: true }` and surfaces a user-facing note (e.g. "no on-chain transaction or Snapshot proposal was submitted"). This file tracks what remains to make each write real.
Convention: each item is `simulateFn` in `catalyrst/sites/packages/data/src/lib/catalyst/<file>` -> the real backend it needs. Tick when the live write path replaces the `simulate*` export.
## Governance (DAO)
- [ ] `simulateDomainStatus` - `governance/submit-catalyst.ts` -> real catalyst domain/health check (the proposal create itself is fail-closed, below)
## Marketplace - on-chain (blocked on the relayer/escrow, your open real-money item)
- [ ] `simulateAccept` - `marketplace/bids.ts` -> accept-bid transaction
- (already fail-closed 501/503 - no fake success: `marketplace/packs.ts` Stripe, `marketplace/tx.ts`)
## Communities (bevy overlay) - federation WRITE path (currently returns 501)
- [ ] `simulateCreateCommunity` - `overlay/create-community.ts` -> social-service-ea federation write
- [ ] `simulateCreate` - `overlay/community-create.ts` (wraps the above)
- [ ] `simulateCommit` (join/leave) - `overlay/community-join.ts` -> federation write (501 pending)
## Admin - moderation backend
- [x] `simulateModerateReport` - `admin/places-moderation.ts` -> moderation write. Live: `machine.ts` defaults `moderate` to the real `moderateDecision` (wrapping `moderateReport`'s PATCH), and no caller overrides it with the simulate variant, so production already hits the real write path.
- [ ] `simulateModerate` - `admin/whatson-admin.ts` -> what's-on moderation write
## Landings - submission backends
- [ ] `simulateSubmitSchedule` - `landings/schedules.ts` -> event-schedule intake
- [ ] `simulateSubmitHangout` - `landings/submit-hangout.ts` -> hangout submission intake
## Fail closed - the simulate* export was removed, replaced by a `failClosed*` default that refuses instead of faking success (goes live by injecting the real commit, no simulation left to swap out)
- `failClosedCreateProposal` - `governance/submit-governance-proposal.ts`, `governance/submit-catalyst.ts`, `governance/submit-council-veto.ts`, `governance/submit-hiring.ts`, `governance/submit-linked-wearables.ts`
- `failClosedCreateTender` - `governance/submit-tender.ts`
- `failClosedSubmitBid` - `governance/submit-bid.ts`
- `failClosedDelegate` - `governance/delegate-vp.ts`
- `failClosedVerify` / `failClosedUnlink` - `governance/link-accounts.ts`
- `failClosedCreate` - `marketplace/sell.ts`
- `failClosedSubmitReport` - `landings/report.ts`
- `failClosedCommit` (machine default) - `features/src/stories/admin/operator-user-bans/machine.ts (the real signed commitUserAction lives in data/src/lib/catalyst/admin/user-bans.ts)`; the real signed `commitUserAction` exists in the same file for callers that inject it
- `savePermissions` returns `Unavailable` via `controlStatus` - `admin/whatson-admin-users.ts`
## Related "unavailable" stubs (fail closed - no action needed beyond backend)
`creator-hub/curate-committee.server.ts` (Discourse committee topic not wired), `creator-hub/deploy-world.server.ts`, `landings/cast-watcher.server.ts` (501 fallback), `builder/collection-detail.ts` (explicit empty stub). (`creator-hub/metrics-funnel.ts` was retired - creator metrics are LIVE via `creator-hub/metrics.server.ts`: real visits/sales/collections data, `null` only when a source fails.)

_Generated from a `grep` of `export ... simulate*` in `catalyrst/sites/packages/data/src/lib/catalyst/` (9 exports remain). Regenerate after wiring any path so the list stays current._
