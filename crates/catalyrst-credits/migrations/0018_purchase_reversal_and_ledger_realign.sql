-- catalyrst-credits: honest reversal accounting + a ledger-faithful origin.
--
-- Two additive changes, both closing live money defects.
--
-- 1. `credit_purchases.revoked_credits`
--
--    A Stripe reversal (charge.refunded / charge.dispute.*) returns the FIAT to
--    the buyer, so the compensating action on our side is to REMOVE the granted
--    Credits. The previous code called the wallet REFUND path, which ADDS
--    credits: the buyer got the money back, kept the credits, and was credited
--    that amount a second time. The reversal path now revokes, and this column
--    is the structural bound on it: cumulative Credits charged back against a
--    purchase can never exceed the Credits that purchase granted.
--
--    The CHECK makes over-revocation unrepresentable rather than merely
--    unlikely, so no future caller can re-open the hole by passing an
--    unbounded amount (the old refund path was unbounded precisely because its
--    clamp keyed on spend rows under the refund's tx_ref, and the reversal
--    passed a Stripe EVENT ID as tx_ref — event ids never carry spend rows).
--
-- 2. Ledger realignment for the 0012 backfill
--
--    reconcile asserts `signed ledger sum == balance`, per wallet, for both the
--    total and the earned bucket. Migration 0012 seeded `earned_available` with
--    `GREATEST(0, LEAST(available, earned_sum))`. For any pre-existing wallet
--    whose earned ledger sum fell outside `[0, available]`, that clamp wrote a
--    balance the ledger cannot reproduce by replay, so reconcile would alarm on
--    those wallets forever through no fault of any later write.
--
--    We record the clamp as an explicit adjustment row rather than teaching
--    reconcile to tolerate a special origin: an equality invariant with a
--    permanent carve-out stops being an invariant, and a tolerance would also
--    mask genuine divergence (e.g. the f64-vs-NUMERIC ledger-row drop fixed in
--    the same change). An adjustment row is self-describing, auditable, and
--    leaves reconcile a pure equality.
--
--    `ports/reconcile.rs::realign_ledger_to_balances` performs the identical
--    operation on demand and is covered by tests; this block is the one-shot
--    historical application of it.
--
-- Additive only; no BEGIN/COMMIT (sqlx wraps each migration in its own tx).

ALTER TABLE credit_purchases
    ADD COLUMN IF NOT EXISTS revoked_credits NUMERIC NOT NULL DEFAULT 0;

ALTER TABLE credit_purchases
    DROP CONSTRAINT IF EXISTS credit_purchases_revoked_bound;
ALTER TABLE credit_purchases
    ADD CONSTRAINT credit_purchases_revoked_bound
    CHECK (revoked_credits >= 0 AND revoked_credits <= credits);

-- Earned-bucket realignment. Positive delta -> a 'grant' row, negative -> a
-- 'consume' row; both are already-valid kinds and reconcile signs them
-- correctly (grant credits, consume debits).
WITH e AS (
    SELECT u.address,
           u.earned_available - COALESCE((
               SELECT SUM(CASE WHEN l.kind IN ('grant','refund','purchase','claim') THEN l.amount
                               WHEN l.kind IN ('spend','consume','expire') THEN -l.amount
                               ELSE 0 END)
               FROM credit_ledger l
               WHERE l.address = u.address AND l.bucket = 'earned'
           ), 0) AS delta
    FROM user_credits u
)
INSERT INTO credit_ledger (address, kind, amount, tx_ref, bucket, captcha_ok)
SELECT address,
       CASE WHEN delta > 0 THEN 'grant' ELSE 'consume' END,
       abs(delta),
       'migration:0018-ledger-realign',
       'earned',
       FALSE
FROM e
WHERE delta <> 0;

-- Total realignment, evaluated AFTER the earned rows above exist so the two
-- adjustments compose instead of fighting. Any residue lands in 'paid', which
-- is by definition `available - earned_available`.
WITH t AS (
    SELECT u.address,
           u.available - COALESCE((
               SELECT SUM(CASE WHEN l.kind IN ('grant','refund','purchase','claim') THEN l.amount
                               WHEN l.kind IN ('spend','consume','expire') THEN -l.amount
                               ELSE 0 END)
               FROM credit_ledger l
               WHERE l.address = u.address
           ), 0) AS delta
    FROM user_credits u
)
INSERT INTO credit_ledger (address, kind, amount, tx_ref, bucket, captcha_ok)
SELECT address,
       CASE WHEN delta > 0 THEN 'grant' ELSE 'consume' END,
       abs(delta),
       'migration:0018-ledger-realign',
       'paid',
       FALSE
FROM t
WHERE delta <> 0;

INSERT INTO admin_audit (action, address, amount, reason, actor, detail)
SELECT 'credits.ledger_realign',
       address,
       SUM(CASE WHEN kind = 'consume' THEN -amount ELSE amount END),
       'migration 0018: record the 0012 earned-bucket clamp as an explicit '
           || 'adjustment so ledger replay reproduces the stored balance',
       'migration',
       jsonb_build_object('migration', '0018', 'txRef', 'migration:0018-ledger-realign')
FROM credit_ledger
WHERE tx_ref = 'migration:0018-ledger-realign'
GROUP BY address;
