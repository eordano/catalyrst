---
id: ffw_rules
status: live
owner: owner@example.com
hypothesis:
  statement: >-
    Telling players when a friend first wears a purchasable wearable -- with a
    one-tap buy link -- converts social proof into shop visits and purchases,
    and an online-aware hourly rate limit keeps it welcome instead of spammy.
  because: >-
    A friend wearing something is the strongest honest signal an item is worth
    owning; capping to one notification per hour unless the player has been
    online since the last one preserves attention for players who are away.
metric:
  primary: ffw_shop_landing
  numerator: ffw_shop_landing
  denominator: ffw_emitted
  guardrails:
    - ffw_suppressed
    - mk_shop_buy_now
  externalEvents:
    - ffw_emitted
    - ffw_suppressed
experiment:
  key: ffw_rules
  unit: recipient
  variants:
    - id: "off"
      weight: 10
      flags:
        rule: "off"
    - id: limit_1h
      weight: 20
      flags:
        rule: limit_1h
    - id: online_bypass
      weight: 60
      flags:
        rule: online_bypass
    - id: unlimited
      weight: 10
      flags:
        rule: unlimited
decision:
  rule: >-
    Server-side delivery-rule test, assigned per recipient ADDRESS by the
    catalyrst-notifications worker (not the sid loader). Keep online_bypass as
    the shipped default unless the funnel shows off/limit_1h matching its
    ffw_shop_landing per ffw_emitted with materially fewer notifications, or
    unlimited shows ffw_suppressed/complaint regressions. Weights here mirror
    the worker's defaults; the live knob is the experiment_overrides row
    (exp_key='ffw_rules'), not this file.
---

# friend_first_wear -- delivery-rule product test

Server-side experiment: the recipient's ADDRESS (not sid) is bucketed with the
shared cyrb53 into a delivery rule, applied by the catalyrst-notifications
`first_wear` worker at emit time.

## Arms (default weights)

| arm | weight | rule |
|---|---|---|
| `off` | 10 | holdout -- never notified |
| `limit_1h` | 20 | hard cap: 1 per recipient per hour |
| `online_bypass` | 60 | 1/hour, but a recipient who fetched notifications since the last one may receive more |
| `unlimited` | 10 | no rate limit (upper bound) |

## Controls

`experiment_overrides` row `exp_key='ffw_rules'` (telemetry DB, dashboard
POST /dash/experiment): `killed` stops ALL emission; `forced_variant` pins
every recipient to one rule; `flags.weights` (`{"off":10,...}`) rebalances.

## Funnel (telemetry_events + notifications DB)

1. `ffw_emitted` {nid, arm, recipient, itemId} -- worker, at insert
2. `ffw_suppressed` {arm, reason, recipient, itemId} -- worker, gated out
3. opened -- `read` flag on the notification row (bell interaction)
4. `ffw_shop_landing` {nid, item_id} -- the buy link carries ?src=ffw&nid=...
5. purchase -- existing mk_shop_buy_now / checkout events, joined by sid+item

Report: `tools/ffw-funnel/report.sh` prints the per-arm funnel.
