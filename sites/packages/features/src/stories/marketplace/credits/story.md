---
id: marketplace-credits
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Surfacing a visitor's live weekly Marketplace Credits balance alongside the
    concrete goals that earn more credits raises the share of sessions that view
    the credits hub and reach a claimable balance versus burying credits behind
    the in-client HUD only.
  because: >-
    Credits are "free MANA" (1 Credit = 1 MANA) but invisible on the web
    marketplace; showing the balance, the expiry countdown, and the exact weekly
    goals up front gives visitors a clear, time-boxed reason to complete goals
    and come back to claim, lifting credits-hub engagement.
metric:
  primary: mk_credits_balance_viewed
  guardrails:
    - mk_credits_viewed
    - mk_credits_goal_viewed
    - mk_credits_claim_clicked
experiment:
  key: marketplace_credits
  unit: session
  variants:
    - id: default
      weight: 100
      flags: {}
  baseline: 0.25
  mde: 0.05
decision:
  rule: >-
    Ship the credits hub if mk_credits_balance_viewed per credits-hub session
    beats the baseline with 95% confidence and no guardrail (mk_credits_viewed,
    mk_credits_goal_viewed, mk_credits_claim_clicked) regresses; otherwise iterate
    on the goal presentation.
---

# Marketplace -- Weekly Credits hub (balance & claim)

Visitor opens the Marketplace Credits hub and sees their live weekly credits
balance, when it expires, and the goals that earn more credits this week.

Journey + metrics (all events carry `story: marketplace-credits`):

- `/marketplace/credits` loads the SSR hub -> `mk_credits_viewed { has_started,
  goal_count, week }` (the surface rendered).
- The balance widget renders -> `mk_credits_balance_viewed { available,
  claimable, expires_in_seconds, blocked }` (the PRIMARY metric -- a visitor
  actually saw their balance).
- The weekly goals list renders -> `mk_credits_goal_viewed { count, claimable,
  completed }`.
- Clicking a goal's "Claim to collect" (or "Claim N Credits") ->
  `mk_credits_claim_clicked { goal, reward }`. The on-chain claim itself is
  SIMULATED here (it requires an auth-chain signed POST /credits/captcha the
  public site cannot mint); the click + intent telemetry is real and the visitor
  is shown the success confirmation.

## Data source

Both reads come from the `catalyrst-credits` crate (confirmed against
`catalyrst-credits/src/lib.rs`):

- `GET /credits/seasons` -> `SeasonsData` (last/current/next season + current
  week countdown).
- `GET /credits/users/{wallet}/progress` -> `CreditsProgramProgressResponse`
  (`user.hasStartedProgram`, `credits` balance, weekly `goals`).

Both routes are gated by an auth-chain signed request (`signer_from`), which the
public SSR site cannot produce, so the loader degrades to the bundled fixture
`app/fixtures/marketplace-credits.json` (faithful to the crate DTOs + the
upstream Unity `CreditsProgramProgressResponse.cs` shapes). `?wallet=0x...` lets a
signed/QA caller exercise the live path.
