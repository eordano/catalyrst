---
id: marketplace-shop
status: implementation
owner: unassigned
hypothesis:
  statement: Returning only visible shop cards and bounding optional backend work reduces time to usable shop content.
  because: Overview previously fetched and priced up to 54 items for 22 card positions, and a failed rail discarded successful catalog results.
metric:
  primary: p95 time to usable shop cards
  guardrails:
    - catalog failure and partial response rates
    - displayed MANA and credit price correctness
    - mk_shop_buy_now
    - backend catalog and quote work
experiment:
  key: marketplace_shop_screen_data
  unit: session
  variants:
    - id: bounded-screen
      weight: 1
      flags: {}
decision:
  rule: Validate request budgets, cancellation, failure isolation and pricing before rollout. Collect cold and warm browser timings before claiming a latency improvement or defining a comparative experiment.
---

# Shop screen data

This is a single implementation, not an A/B experiment. No measured baseline,
effect size, sample size or assigned product owner is claimed. Existing
`marketplace/shop` interaction events keep their current meaning.

The `/shop` SSR loader and `GET /api/screens/v1/shop` share
`packages/data/src/lib/screens/shop.server.ts`. SSR calls it directly, without
an extra HTTP hop. The resource returns `{ version: 1, filters, cards, topCards,
trendingCards, total, fallback, sections }`; each catalog section is `ready`,
`unavailable`, or `skipped`. Successful empty sections remain distinguishable
from failures. The public resource contains no session id or user-specific data.

Overview requests up to 8 main cards, 6 ranking cards and 8 deal cards. Browse
retains 40-item pagination. The backend's existing `minPrice=1` filter excludes
zero-price-only entries before pagination; frontend buyability validation remains
in place. Only displayed, deduplicated items are quoted. Account and favorites
tabs perform no catalog or quote reads. Unknown tabs normalize to overview and
hidden overview pagination resets to zero.

Each main catalog request has a 3-second deadline. A screen waits at most
1 second for each public rail, while the shared refresh has its own 3-second
deadline and can populate the cache for another visitor. Quotes have a separate
1-second deadline, falling back to the existing MANA price display. These are
initial operational limits, not measured latency targets. Normal checkout still
validates prices through the existing authoritative path.

The existing 60-second rail cache coalesces refreshes. Visitor cancellation
does not cancel a shared refresh. SSR responses are private/no-store and the
resource initially uses no-store; no whole-response public caching is introduced.
`Server-Timing` reports catalog, ranking, deals, quotes and total composition time.

Verification lives in `screens/shop.server.test.ts` and the shop route tests.
The Routes/Shop catalog includes both total failure and ranking-only failure.
Browser LCP/INP and real-network latency comparisons remain rollout work; request
budget reductions alone are not evidence of a measured user-visible speedup.
