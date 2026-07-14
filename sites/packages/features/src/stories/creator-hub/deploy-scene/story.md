---
id: creator-hub-deploy-scene
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided publish-to-World destination path (choose Worlds -> pick a NAME ->
    review files against the 50MB quota -> deploy) raises the share of started
    World publishes that reach the deploy/confirm step, even with the upload
    simulated.
  because: >-
    Making the World destination explicit -- surfacing the wallet's claimable
    NAMEs (or a clear Claim-a-NAME path when there are none) and the file list
    vs the 50MB quota before any upload -- removes the two biggest sources of
    uncertainty (where it lands, will it fit), so more creators who start a
    World publish push through to deploy instead of bailing.
metric:
  primary: ch_deploy_world_confirm_rate
  numerator: ch_deploy_world_confirm_reached
  denominator: ch_deploy_world_started
  guardrails:
    - ch_deploy_world_started
    - ch_deploy_world_names_empty
    - ch_deploy_world_quota_exceeded
experiment:
  key: ch_deploy_world_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.45
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if ch_deploy_world_confirm_rate improves by at least the MDE with no
    guardrail regression (World-publish start volume holds, the empty-NAMEs path
    stays graceful, and the quota-exceeded rate does not spike); otherwise hold.
---

# Deploy a scene to a World

The full publish-to-World destination path in the Creator Hub PublishProject
wizard (`/creator-hub/deploy-world`). It extends the generic publish wizard,
which today stops at a generic target, into a World-specific funnel:

1. **destination** -- choose Worlds (vs LAND).
2. **select-world** -- pick an ENS provider + a World NAME, OR hit the
   empty-NAMEs state with a *Claim a NAME* affordance.
3. **review** -- the file list + total size vs the 50MB World quota; confirm.
4. **deploying** -- simulated upload/convert/optimize (ConnectedSteps progress).
5. **complete** -- success tick + the realm jump-in URL + copy.
6. **error** -- the publishing-failed state with retry / report.

- **Primary metric:** `ch_deploy_world_confirm_rate` =
  `ch_deploy_world_confirm_reached` / `ch_deploy_world_started`.
- **Guardrails:** World-publish start volume (`ch_deploy_world_started`), the
  empty-NAMEs path (`ch_deploy_world_names_empty`), and the over-quota path
  (`ch_deploy_world_quota_exceeded`) must all stay healthy.
- **Events:** `ch_deploy_world_started` on entry,
  `ch_deploy_world_destination_selected` (`{target}`),
  `ch_deploy_world_name_selected` (`{name}`) | `ch_deploy_world_names_empty`,
  `ch_deploy_world_review_reached` (`{total_bytes, exceeded}`),
  `ch_deploy_world_quota_exceeded` (guardrail, when over 50MB),
  `ch_deploy_world_confirm_reached`, `ch_deploy_world_completed` (`{name,
  jump_url}`), `ch_deploy_world_failed` on the error state.

## Data reality

`GET /lambdas/users/{address}/names` is **live (200)** and returns 0 claimable
World NAMEs for the wallet under test -- so the **empty-NAMEs branch is the real,
default state** (Claim a NAME). The health chip probes `GET /status` on the
Worlds server -- the same host deploys POST to. On **Confirm** the wizard
runs a **REAL deploy**: the route reads the creator's on-disk scene folder (File
System Access), CIDv1-hashes every file (multi-block dag-pb for >256KiB assets),
signs an `ECDSA_SIGNED_ENTITY` AuthChain with the connected wallet's identity,
and POSTs the multipart body to the worlds-content-server `/entities`. When no
real deploy is wired (no `deploy` prop), Confirm lands on the honest terminal
`unavailable` state instead of fabricating success. The populated SelectWorld
branch uses fixture-derived NAMEs (faithful to `schemas` `WorldConfiguration.name`
+ the worlds-content-server index shape) so both the empty and selection paths
stay screenshot-addressable.
