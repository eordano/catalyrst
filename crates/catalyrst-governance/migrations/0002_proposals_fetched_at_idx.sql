-- no-transaction
-- list_proposals orders every page by fetched_at DESC; id second so a tiebreaker can use the same index.
-- CONCURRENTLY needs the no-transaction directive above (sqlx wraps every other migration in one).
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_proposals_fetched_at ON proposals (fetched_at DESC, id);
