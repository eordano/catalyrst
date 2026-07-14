# H2-01 · 1m paint-tile grid fits the entity and load budget

- **IF/THEN:** IF the paint grid drops from 2m×2m to 1m×1m (≈4× more tiles), THEN scene load time stays ≤1.5× of V0 and the entity count stays inside SDK7 scene limits.
- **Source section:** §6 Mobile-First — performance risk; §9 plan Week 1
- **Cheapest killing test:** arithmetic in the doc first (tile count × entities-per-tile vs current SDK7 limits — look the limits up, do not quote from memory), then a load-time measurement on Stom's refactored branch in desktop Explorer
- **Key metric:** scene load time at 1m grid vs V0 baseline (fail: >1.5×)
- **Mobile-sensitive:** yes
- **Tested on:** —
- **Parked:** 2026-08-12

## Brief
<!-- owned by /pre-prod-proto -->

## Sessions
<!-- owned by /pre-prod-proto -->

## Verdict
<!-- owned by /pre-prod-proto -->
