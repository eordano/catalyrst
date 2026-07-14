-- Remove the seasons domain (legacy). Earned credits become NON-EXPIRING.
--
-- Seasons, weeks, and goals are gone: goals were structurally season-scoped
-- (credits_goals.week_id -> credits_weeks.season_id -> credits_seasons), so the
-- whole subtree goes, children first. user_goal_events (0011) and
-- user_goal_progress (0001) reference credits_goals.
--
-- user_credits.earned_expires_at stays as a column (0012's applied bytes and
-- backfill remain historically valid) but is NULLed: no code writes it anymore
-- and the earned slice never expires. Historical 'expire' rows in credit_ledger
-- remain valid; reconcile math still counts them as debits.

-- credit_ledger.week_id (0001) references credits_weeks; keep the column as
-- plain historical data but drop the FK so the table can go.
ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_week_id_fkey;

DROP TABLE IF EXISTS user_goal_events;
DROP TABLE IF EXISTS user_goal_progress;
DROP TABLE IF EXISTS credits_goals;
DROP TABLE IF EXISTS credits_weeks;
DROP TABLE IF EXISTS credits_seasons;

UPDATE user_credits
   SET earned_expires_at = NULL
 WHERE earned_expires_at IS NOT NULL;
