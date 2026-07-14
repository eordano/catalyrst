-- Player abuse reports — the catalyrst equivalent of report-user.decentraland.org.
-- The reporter is always the signed-fetch signer; the client-supplied address is ignored.
--
-- Evidence bytes live in postgres rather than an object store: this stack has no S3
-- (catalyrst-storage is a content-addressed blob helper, catalyrst-media only transcodes),
-- and the wizard caps evidence at 5 files x 10 MB, which TOAST handles without new config.
-- Point this at an object store if the cap ever grows.
CREATE TABLE IF NOT EXISTS player_reports (
    id                  UUID PRIMARY KEY,
    reporter_address    VARCHAR(42) NOT NULL,
    reported_address    VARCHAR(42) NOT NULL,
    reason              TEXT NOT NULL,
    description         TEXT NOT NULL,
    additional_comments TEXT,
    evidence_keys       TEXT[] NOT NULL DEFAULT '{}',
    status              TEXT NOT NULL DEFAULT 'open',
    created_at          TIMESTAMP NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_player_reports_reported
    ON player_reports (reported_address, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_player_reports_reporter
    ON player_reports (reporter_address, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_player_reports_status
    ON player_reports (status, created_at DESC);

-- One row per evidence slot, minted by /reports/players/presign before any bytes exist.
-- A slot with uploaded_at IS NULL and no matching player_reports row is an abandoned
-- draft and is swept on the next presign.
CREATE TABLE IF NOT EXISTS player_report_evidence (
    report_id        UUID NOT NULL,
    evidence_key     TEXT NOT NULL,
    reporter_address VARCHAR(42) NOT NULL,
    filename         TEXT NOT NULL,
    content_type     TEXT NOT NULL,
    declared_size    BIGINT NOT NULL,
    content          BYTEA,
    created_at       TIMESTAMP NOT NULL DEFAULT now(),
    uploaded_at      TIMESTAMP,
    PRIMARY KEY (report_id, evidence_key)
);

CREATE INDEX IF NOT EXISTS idx_player_report_evidence_reporter
    ON player_report_evidence (reporter_address, created_at);
