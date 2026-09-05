-- catalyrst-credits initial schema
-- Marketplace Credits program: seasons/weeks/goals config + per-address
-- enrollment, balance, goal progress, and an append-only credit ledger.
-- Mirrors the shape consumed by unity-explorer's MarketplaceCreditsAPIClient.

CREATE TABLE IF NOT EXISTS credits_seasons (
    id              SERIAL PRIMARY KEY,
    name            TEXT NOT NULL,
    start_date      TIMESTAMPTZ NOT NULL,
    end_date        TIMESTAMPTZ NOT NULL,
    max_mana        NUMERIC NOT NULL DEFAULT 0,
    amount_of_weeks INT NOT NULL DEFAULT 0,
    state           TEXT NOT NULL DEFAULT 'NOT_STARTED'
);

CREATE TABLE IF NOT EXISTS credits_weeks (
    id          SERIAL PRIMARY KEY,
    season_id   INT NOT NULL REFERENCES credits_seasons(id) ON DELETE CASCADE,
    week_number INT NOT NULL,
    start_date  TIMESTAMPTZ NOT NULL,
    end_date    TIMESTAMPTZ NOT NULL,
    UNIQUE (season_id, week_number)
);

CREATE TABLE IF NOT EXISTS credits_goals (
    id          SERIAL PRIMARY KEY,
    week_id     INT NOT NULL REFERENCES credits_weeks(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    thumbnail   TEXT NOT NULL DEFAULT '',
    reward      NUMERIC NOT NULL DEFAULT 0,
    total_steps INT NOT NULL DEFAULT 1,
    sort_order  INT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_credits_goals_week ON credits_goals(week_id);

-- per-wallet enrollment in the program (address is lowercase 0x...)
CREATE TABLE IF NOT EXISTS user_program (
    address              TEXT PRIMARY KEY,
    has_started_program  BOOLEAN NOT NULL DEFAULT TRUE,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- per-wallet running balance
CREATE TABLE IF NOT EXISTS user_credits (
    address                  TEXT PRIMARY KEY,
    available                NUMERIC NOT NULL DEFAULT 0,
    expires_at               TIMESTAMPTZ,
    is_blocked_for_claiming  BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- per-wallet goal progress
CREATE TABLE IF NOT EXISTS user_goal_progress (
    address         TEXT NOT NULL,
    goal_id         INT NOT NULL REFERENCES credits_goals(id) ON DELETE CASCADE,
    completed_steps INT NOT NULL DEFAULT 0,
    is_claimed      BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (address, goal_id)
);

-- append-only audit of grants / claims / expiries / consumption
CREATE TABLE IF NOT EXISTS credit_ledger (
    id         BIGSERIAL PRIMARY KEY,
    address    TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('grant', 'claim', 'expire', 'consume')),
    amount     NUMERIC NOT NULL,
    week_id    INT REFERENCES credits_weeks(id) ON DELETE SET NULL,
    captcha_ok BOOLEAN NOT NULL DEFAULT FALSE,
    tx_ref     TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_credit_ledger_address ON credit_ledger(address);

-- short-lived per-wallet captcha challenges for the claim flow.
-- answer_x is the expected solution; image bytes are regenerated on read.
CREATE TABLE IF NOT EXISTS captcha_challenges (
    id          BIGSERIAL PRIMARY KEY,
    address     TEXT NOT NULL,
    answer_x    NUMERIC NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_captcha_address_active
    ON captcha_challenges(address) WHERE consumed_at IS NULL;
