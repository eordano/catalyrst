-- Referral attribution (upstream social-service-ea referral_progress): one row per
-- invited user, first referrer wins; status walks pending -> signed_up -> tier_granted.
CREATE TABLE IF NOT EXISTS referral_progress (
    invited_user TEXT PRIMARY KEY,
    referrer     TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'signed_up', 'tier_granted', 'rejected_ip_match')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS referral_progress_referrer_idx
    ON referral_progress (referrer);

-- Last invitedUsersAccepted count each referrer has seen; GET /v1/referral-progress
-- reports the stored value and writes back the current count.
CREATE TABLE IF NOT EXISTS referral_progress_viewed (
    referrer                TEXT PRIMARY KEY,
    invites_accepted_viewed BIGINT NOT NULL DEFAULT 0,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);
