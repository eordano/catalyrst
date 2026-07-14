# Simulated write-paths - real-backend checklist
Reads are real across the site; the write / action paths below are disclosed simulations - each returns `{ simulated: true }` / `{ stub: true }` and surfaces a user-facing note (e.g. "no on-chain transaction or Snapshot proposal was submitted"). This file tracks what remains to make each write real.
Convention: each item is `simulateFn` in `catalyrst/sites/packages/data/src/lib/catalyst/<file>` -> the real backend it needs. Tick when the live write path replaces the `simulate*` export.
## Governance (DAO) - Snapshot proposal + wallet signature
- [ ] `simulateCreateProposal` - `governance/submit-governance-proposal.ts` -> Snapshot proposal create + wallet sign
- [ ] `simulateCreateProposal` / `simulateDomainStatus` - `governance/submit-catalyst.ts` -> Snapshot + real catalyst domain/health check
- [ ] `simulateCreateProposal` - `governance/submit-council-veto.ts` -> Snapshot
- [ ] `simulateCreateProposal` - `governance/submit-hiring.ts` -> Snapshot
- [ ] `simulateCreateProposal` - `governance/submit-linked-wearables.ts` -> Snapshot
- [ ] `simulateCreateProposalTender` - `governance/submit-tender.ts` -> Snapshot
- [ ] `simulateCreateBid` - `governance/submit-bid.ts` -> Snapshot bid proposal
- [ ] `simulateDelegate` - `governance/delegate-vp.ts` -> Snapshot delegation registry (external, non-catalyst)
- [ ] `simulateVerify` / `simulateUnlink` - `governance/link-accounts.ts` -> account-link backend / on-chain
## Marketplace - on-chain (blocked on the relayer/escrow, your open real-money item)
- [ ] `simulateSignAndCreate` - `marketplace/sell.ts` -> sign + Marketplace contract listing create
- [ ] `simulateAccept` - `marketplace/bids.ts` -> accept-bid transaction
- (already fail-closed 501/503 - no fake success: `marketplace/packs.ts` Stripe, `marketplace/tx.ts`)
## Communities (bevy overlay) - federation WRITE path (currently returns 501)
- [ ] `simulateCreateCommunity` - `overlay/create-community.ts` -> social-service-ea federation write
- [ ] `simulateCreate` - `overlay/community-create.ts` (wraps the above)
- [ ] `simulateCommit` (join/leave) - `overlay/community-join.ts` -> federation write (501 pending)
## Admin - moderation backend
- [x] `simulateModerateReport` - `admin/places-moderation.ts` -> moderation write. Live: `machine.ts` defaults `moderate` to the real `moderateDecision` (wrapping `moderateReport`'s PATCH), and no caller overrides it with the simulate variant, so production already hits the real write path.
- [ ] `simulateUserAction` (ban/kick/warn) - `admin/user-bans.ts`
- [ ] `simulateModerate` - `admin/whatson-admin.ts`
- [ ] `simulateSavePermissions` - `admin/whatson-admin-users.ts`
## Landings - submission backends
- [ ] `simulateSubmitReport` - `landings/report.ts` -> abuse-report intake
- [ ] `simulateSubmitSchedule` - `landings/schedules.ts` -> event-schedule intake
- [ ] `simulateSubmitHangout` - `landings/submit-hangout.ts` -> hangout submission intake
## Related "unavailable" stubs (fail closed - no action needed beyond backend)
`creator-hub/curate-committee.server.ts` (Discourse committee topic not wired), `creator-hub/deploy-world.server.ts`, `landings/cast-watcher.server.ts` (501 fallback), `builder/collection-detail.ts` (explicit empty stub). (`creator-hub/metrics-funnel.ts` was retired - creator metrics are LIVE via `creator-hub/metrics.server.ts`: real visits/sales/collections data, `null` only when a source fails.)

_Generated from a `grep` of `export ... simulate*` in `catalyrst/sites/packages/data/src/lib/catalyst/`. Regenerate after wiring any path so the list stays current._
