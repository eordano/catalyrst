-- Dedicated unconfirmed-email staging table, mirroring upstream
-- notifications-workers' `unconfirmed_emails`.
--
-- The set-email flow writes the pending email + a 32-char confirmation `code`
-- here (one row per address) and renders/sends the confirmation email out of
-- band. confirm-email promotes the row into `subscriptions.email` and deletes
-- it. Keeping this off the `subscriptions` row means an unconfirmed change
-- never overwrites the already-confirmed address until the code is presented.

CREATE TABLE IF NOT EXISTS unconfirmed_emails (
    address     TEXT PRIMARY KEY,
    email       TEXT NOT NULL,
    code        TEXT NOT NULL,
    -- credits workflow uses a different confirmation template + redirect base.
    source      TEXT NOT NULL DEFAULT 'account',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Cross-account uniqueness lookups join confirmed addresses on lowercased
-- email, so index `subscriptions.email`.
CREATE INDEX IF NOT EXISTS idx_subscriptions_email
    ON subscriptions (lower(email))
    WHERE email IS NOT NULL;
