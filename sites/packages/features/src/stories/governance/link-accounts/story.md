---
id: governance-link-accounts
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Guiding account linking through an explicit, deep-linkable wizard (choose
    account -> connect (Sign/Copy/Post) -> verifying -> connected) increases the
    share of started linkings that reach a connected account, even with the
    signature verification and the link/unlink writes simulated.
  because: >-
    Breaking identity linking into legible, ordered steps with a visible
    time-sensitive verification reduces the uncertainty that makes users abandon
    the opaque, modal-only connect flow, so more people who start a link push
    through to the connected state instead of bailing mid-signature.
metric:
  primary: gv_link_connected_rate
  guardrails:
    - gv_link_started
    - gv_link_verify_error
    - gv_link_unlinked
experiment:
  key: gv_link_accounts_wizard
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
    Ship if gv_link_connected_rate improves by at least the MDE with no guardrail
    regression (link-start volume holds, verification-error rate does not climb,
    and the unlink path stays graceful); otherwise hold.
---

# Link external accounts (Discord / Forum / Push)

The account-linking flow (`/governance/profile/link-accounts`) breaks linking an
external identity into explicit, URL-addressable steps: choose an account
(Discord / Forum / Push), walk the connection flow (the Forum/Discord 3-step
Sign -> Copy -> Post `FlowWithSteps`, or the Push subscribe), wait through a
time-sensitive signature verification, and land on the connected (PostConnection)
success screen. An already-linked account can be unlinked from the chooser via
`?account=<id>&action=unlink` (UnlinkAccountCard confirm).

This story tracks whether the wizard increases the share of started linkings that
reach a connected account. Governance is **not** a Catalyst service (the DAO runs
on Snapshot + Aragon externally), and the linking backend is signature-gated, so
the **verification timer and the link/unlink writes are SIMULATED** via an XState
machine -- the flow, states, and metrics are real; the final commit is a clearly
noted stub.

- **Primary metric:** `gv_link_connected_rate` = `gv_link_connected` / `gv_link_started`.
- **Guardrails:** link-start volume (`gv_link_started`), verification failures
  (`gv_link_verify_error`), and the unlink path (`gv_link_unlinked`) must stay
  healthy.
- **Events:** `gv_link_started` (`{account}`) on choosing an account,
  `gv_link_connect_step` (`{account, step}`) per connection step,
  `gv_link_verifying` (`{account}`) entering verification,
  `gv_link_connected` (`{account, stub}`) on success,
  `gv_link_verify_error` (`{account}`) on a failed verification, and
  `gv_link_unlinked` (`{account, stub}`) when an account is unlinked.

Copy/labels/steps are verbatim from the live decentraland/governance i18n bundle
(`modal.identity_setup.*`); see `app/fixtures/governance-link-accounts.json`
`_source`.
