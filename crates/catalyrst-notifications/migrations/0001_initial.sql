-- catalyrst-notifications initial schema.
--
-- Fresh design backing the SignedFetch reader/marker surface of
-- notifications.decentraland.org. The notification rows themselves are seeded
-- by external writers (the deferred ingestion path); v1 only reads/marks them.
-- All queries are scoped by the recovered signer address.

CREATE TABLE IF NOT EXISTS notifications (
    id                  UUID PRIMARY KEY,
    address             TEXT NOT NULL,
    type                TEXT NOT NULL,
    metadata            JSONB NOT NULL DEFAULT '{}'::jsonb,
    broadcast_address   TEXT,
    -- unix epoch in milliseconds (the explorer polling cursor `from` is ms)
    timestamp           BIGINT NOT NULL,
    read                BOOLEAN NOT NULL DEFAULT FALSE,
    read_at             BIGINT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_notifications_address_timestamp
    ON notifications (address, timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_notifications_address_unread
    ON notifications (address)
    WHERE read = FALSE;

-- Email subscription record, one row per address.
-- `details` mirrors the @dcl/schemas SubscriptionDetails shape:
--   { ignore_all_email, ignore_all_in_app, message_type: { <type>: { email, in_app } } }
CREATE TABLE IF NOT EXISTS subscriptions (
    address                     TEXT PRIMARY KEY,
    email                       TEXT,
    unconfirmed_email           TEXT,
    email_confirmation_token    TEXT,
    details                     JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_credits_workflow         BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Per-scope opt-outs (currently scope='community', scope_id=<communityId>).
CREATE TABLE IF NOT EXISTS subscription_opt_outs (
    address     TEXT NOT NULL,
    scope       TEXT NOT NULL,
    scope_id    TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (address, scope, scope_id)
);
