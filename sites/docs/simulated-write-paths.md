# Simulated write-paths - real-backend checklist
Simulation exports are not a production capability inventory: routes can inject
real actors, refuse an action, or accidentally inherit a preview default. Review
the route and actor together. See [product capabilities](product-capabilities.md)
for the common assumptions and source evidence. This checklist records remaining
preview implementations; neither all reads nor all error paths are uniformly live.
## Governance (DAO)
- [ ] `simulateDomainStatus` - `governance/submit-catalyst.ts` -> real catalyst domain/health check (the proposal create itself is fail-closed, below)
## Marketplace - on-chain (blocked on the relayer/escrow, your open real-money item)
- [ ] `simulateAccept` - `marketplace/bids.ts` -> accept-bid transaction
- `marketplace/packs.ts` refuses unavailable Stripe checkout. `marketplace/tx.ts` prepares signing payloads; preparation alone is not transaction submission. Secondary-purchase and NAME routes explicitly refuse unavailable completion paths.
## Communities (bevy overlay) - preview exports and production wiring
- [ ] `simulateCreateCommunity` - `overlay/create-community.ts` -> social-service-ea federation write
- [ ] `simulateCreate` - `overlay/community-create.ts` (wraps the above)
- [x] Join/request production wiring uses `buildCommunityJoinCommit` in `overlay/community-commit.ts`. Anonymous writes refuse; signed POSTs distinguish membership from a pending private request. `simulateCommit` remains a standalone preview export, not the production fallback.
## Admin - moderation backend
- [x] `simulateModerateReport` - `admin/places-moderation.ts` -> moderation write. Live: `machine.ts` defaults `moderate` to the real `moderateDecision` (wrapping `moderateReport`'s PATCH) and no caller overrides it with the simulate variant, so production already hits the real write path.
- [ ] `simulateModerate` - `admin/whatson-admin.ts` -> what's-on moderation write
## Landings - submission backends
- [ ] `simulateSubmitSchedule` - `landings/schedules.ts` -> event-schedule intake
- [ ] `simulateSubmitHangout` - `landings/submit-hangout.ts` -> hangout submission intake
## Verified route corrections (2026-09-17)

- RSVP/cancellation use signed POST/DELETE attendee writes and resolve identity at
  action time. Production results are not labelled as simulated.
- Notification preferences use signed GET/PUT `/subscription`; saving preferences
  does not verify an email address or prove delivery.
- Secondary NFT purchases and NAME registration refuse without requesting a
  signature. Signature-only success and signature-derived token IDs were removed.
  Production URL step previews cannot claim completion.

## Fail closed - the simulate* export was removed, replaced by a `failClosed*` default that refuses instead of faking success (goes live by injecting the real commit, no simulation left to swap out)
- `failClosedCreateProposal` - `governance/submit-governance-proposal.ts`, `governance/submit-catalyst.ts`, `governance/submit-council-veto.ts`, `governance/submit-hiring.ts`, `governance/submit-linked-wearables.ts`
- `failClosedCreateTender` - `governance/submit-tender.ts`
- `failClosedSubmitBid` - `governance/submit-bid.ts`
- `failClosedDelegate` - `governance/delegate-vp.ts`
- `failClosedVerify` / `failClosedUnlink` - `governance/link-accounts.ts`
- `failClosedCreate` - `marketplace/sell.ts`
- `failClosedSubmitReport` - `landings/report.ts`
- `failClosedCommit` (machine default) - `features/src/stories/admin/operator-user-bans/machine.ts`; the real signed `commitUserAction` lives in `data/src/lib/catalyst/admin/user-bans.ts` for callers that inject it
- `savePermissions` returns `Unavailable` via `controlStatus` - `admin/whatson-admin-users.ts`
## Related "unavailable" stubs (fail closed - no action needed beyond backend)
`creator-hub/curate-committee.server.ts` (Discourse committee topic not wired), `creator-hub/deploy-world.server.ts`, `landings/cast-watcher.server.ts` (501 fallback), `builder/collection-detail.ts` (explicit empty stub). `creator-hub/metrics-funnel.ts` was retired - creator metrics are LIVE via `creator-hub/metrics.server.ts`: real visits/sales/collections data, `null` only when a source fails.
