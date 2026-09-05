-- catalyrst-market admin moderation + audit schema (docs/admin-console.md §4).
--
-- These tables live alongside the federation log in the `marketplace` schema
-- (the search_path that `build_state` SETs on the write pool). They are
-- OPERATOR-OWNED state: every row is authored by a bearer-authenticated admin,
-- not by an EIP-712 signature. They never mutate the existing federation log
-- rows in place — moderation flags and disputes are separate side tables keyed
-- by the target's signature_hash, and force-cancel APPENDS an operator-authored
-- row to the existing `market_cancellations` log so it propagates over the
-- /federation/market/changes feed exactly like a wallet-signed cancellation.
--
-- All three tables are additive; no existing read/write path changes behavior.

-- Append-only audit trail: who (admin actor) did what, when, to which target.
-- Keyed by a surrogate BIGSERIAL; never updated, only inserted.
CREATE TABLE IF NOT EXISTS market_admin_audit (
    id           BIGSERIAL  PRIMARY KEY,
    actor        TEXT       NOT NULL,          -- authenticated admin identity
    action       TEXT       NOT NULL,          -- e.g. flag.set, flag.clear, dispute.open, dispute.resolve, listing.force_cancel
    target_kind  TEXT       NOT NULL,          -- bid | order | trade
    target_hash  TEXT       NOT NULL,          -- signature_hash of the target row
    detail       JSONB      NOT NULL DEFAULT '{}'::jsonb,
    created_at   BIGINT     NOT NULL           -- unix seconds, server clock at write time
);

CREATE INDEX IF NOT EXISTS idx_maa_target  ON market_admin_audit (target_kind, target_hash, id DESC);
CREATE INDEX IF NOT EXISTS idx_maa_actor   ON market_admin_audit (actor, id DESC);
CREATE INDEX IF NOT EXISTS idx_maa_created ON market_admin_audit (created_at DESC);

-- Moderation flag on a single local listing (bid/order) or trade. One active
-- flag per target (PK on target_hash). Clearing a flag DELETEs the row; the
-- audit table retains the history. `severity` is advisory metadata for the
-- console; the presence of a row is the moderation signal.
CREATE TABLE IF NOT EXISTS market_moderation_flags (
    target_hash  TEXT       PRIMARY KEY,
    target_kind  TEXT       NOT NULL CHECK (target_kind IN ('bid','order','trade')),
    severity     TEXT       NOT NULL DEFAULT 'review' CHECK (severity IN ('review','hide','block')),
    reason       TEXT       NOT NULL DEFAULT '',
    flagged_by   TEXT       NOT NULL,
    flagged_at   BIGINT     NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mmf_kind     ON market_moderation_flags (target_kind, flagged_at DESC);
CREATE INDEX IF NOT EXISTS idx_mmf_severity ON market_moderation_flags (severity);

-- Dispute lifecycle for a recorded trade. One dispute per trade signature_hash.
-- Status transitions open -> (resolved | rejected) are operator-driven and
-- recorded in the audit table.
CREATE TABLE IF NOT EXISTS market_disputes (
    trade_hash    TEXT       PRIMARY KEY,
    status        TEXT       NOT NULL DEFAULT 'open' CHECK (status IN ('open','resolved','rejected')),
    reason        TEXT       NOT NULL DEFAULT '',
    resolution    TEXT       NOT NULL DEFAULT '',
    opened_by     TEXT       NOT NULL,
    opened_at     BIGINT     NOT NULL,
    resolved_by   TEXT,
    resolved_at   BIGINT
);

CREATE INDEX IF NOT EXISTS idx_mdisp_status ON market_disputes (status, opened_at DESC);
