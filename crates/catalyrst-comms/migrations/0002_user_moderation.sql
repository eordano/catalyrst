ALTER TABLE user_bans
    ADD COLUMN IF NOT EXISTS lifted_at  TIMESTAMP,
    ADD COLUMN IF NOT EXISTS lifted_by  VARCHAR,
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMP NOT NULL DEFAULT now();

CREATE INDEX IF NOT EXISTS idx_user_bans_lifted
    ON user_bans (lifted_at, expires_at);
