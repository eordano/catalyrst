-- Stores the latest connection information of each player, captured (best-effort)
-- whenever a LiveKit token is requested on /get-scene-adapter or /private-messages/token.
-- One row per player (keyed by address), upserted with the most recent IP address
-- (from the Cloudflare cf-connecting-ip header) and device id (from the signed-fetch auth
-- metadata deviceIdentifier). Both are nullable: older clients may not send a device id and
-- the IP header may be absent. Timestamps are milliseconds since epoch.
CREATE TABLE IF NOT EXISTS player_connection_info (
    address     VARCHAR(42) PRIMARY KEY,
    -- Max length covers IPv6 addresses.
    ip_address  VARCHAR(45),
    device_id   TEXT,
    created_at  BIGINT NOT NULL,
    updated_at  BIGINT NOT NULL
);

-- Snapshot, at ban time, of the player's device id (read from player_connection_info).
-- Lets us reject a banned player who reconnects from the same device under a different
-- wallet. Nullable: a player with no recorded connection info is still banned by address.
ALTER TABLE user_bans
    ADD COLUMN IF NOT EXISTS banned_device_id TEXT;

-- Partial index so the per-token-request ban lookup stays fast and only scans
-- currently-active bans that actually carry a device id.
CREATE INDEX IF NOT EXISTS idx_user_bans_banned_device_id_active
    ON user_bans (banned_device_id)
    WHERE lifted_at IS NULL AND banned_device_id IS NOT NULL;
