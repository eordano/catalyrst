-- Seed the buyable Credit packs at the canonical 0.10 USD/Credit peg
-- (10 Credits per USD — see ports/pricing.rs CREDIT_USD). Four tiers:
-- $5 / $15 / $25 / $50 -> 50 / 150 / 250 / 500 Credits.
--
-- Idempotent: deactivate any pre-existing packs, then upsert exactly these
-- four as active. Runs once (sqlx tracks applied migrations); re-applying the
-- same tiers via ON CONFLICT keeps them in sync without duplicating.

UPDATE credit_packs SET active = FALSE;

INSERT INTO credit_packs (sku, title, credits, price_cents, currency, active, sort_order) VALUES
    ('credits_usd_5',  'Starter',  50,   500, 'usd', TRUE, 1),
    ('credits_usd_15', 'Plus',    150,  1500, 'usd', TRUE, 2),
    ('credits_usd_25', 'Pro',     250,  2500, 'usd', TRUE, 3),
    ('credits_usd_50', 'Max',     500,  5000, 'usd', TRUE, 4)
ON CONFLICT (sku) DO UPDATE SET
    title       = EXCLUDED.title,
    credits     = EXCLUDED.credits,
    price_cents = EXCLUDED.price_cents,
    currency    = EXCLUDED.currency,
    active      = TRUE,
    sort_order  = EXCLUDED.sort_order;
