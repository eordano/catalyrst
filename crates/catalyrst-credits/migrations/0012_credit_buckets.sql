-- Earned/paid credit split. `available` stays the TOTAL spendable balance
-- (wire + checkout compat); `earned_available` is the slice of it that came
-- from weekly-goal claims and expires at end-of-season. paid = available -
-- earned_available and never expires. Every ledger row is tagged with the
-- bucket it moved.
ALTER TABLE credit_ledger
    ADD COLUMN bucket TEXT NOT NULL DEFAULT 'paid';
ALTER TABLE credit_ledger
    ADD CONSTRAINT credit_ledger_bucket_check CHECK (bucket IN ('earned', 'paid'));

-- Historic rows: goal claims are the only earned source so far.
UPDATE credit_ledger SET bucket = 'earned' WHERE kind = 'claim';

ALTER TABLE user_credits
    ADD COLUMN earned_available NUMERIC NOT NULL DEFAULT 0;
ALTER TABLE user_credits
    ADD COLUMN earned_expires_at TIMESTAMPTZ;

-- Backfill: a wallet's earned slice is its signed earned-bucket ledger sum,
-- bounded to [0, available]; live earned credits expire with the current
-- season (there were no 'expire' rows before this migration).
WITH e AS (
    SELECT address,
           SUM(CASE WHEN kind IN ('grant','refund','purchase','claim') THEN amount
                    WHEN kind IN ('spend','consume','expire') THEN -amount
                    ELSE 0 END) AS earned_sum
    FROM credit_ledger
    WHERE bucket = 'earned'
    GROUP BY address
)
UPDATE user_credits u
SET earned_available  = GREATEST(0, LEAST(u.available, e.earned_sum)),
    earned_expires_at = CASE
        WHEN GREATEST(0, LEAST(u.available, e.earned_sum)) > 0 THEN
            (SELECT end_date FROM credits_seasons
             WHERE start_date <= now() AND end_date >= now()
             ORDER BY start_date DESC LIMIT 1)
        ELSE NULL END
FROM e
WHERE e.address = u.address;

ALTER TABLE user_credits
    ADD CONSTRAINT user_credits_earned_bound
    CHECK (earned_available >= 0 AND earned_available <= available);
