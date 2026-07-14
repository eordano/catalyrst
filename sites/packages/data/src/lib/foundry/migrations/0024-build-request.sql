-- 0024-build-request -- a recorded ask to build a design doc into a playable
-- scene, and the broker's status ledger for that ask. scene_slug is NOT an FK:
-- the foundry.scene row exists only after the build lands (registerScene mints
-- it). Idempotent; mirrored as end-state into schema.sql (the lockstep rule).
DO $mig$
BEGIN
  IF to_regclass('foundry.gdd_doc') IS NULL THEN
    RAISE NOTICE '0024-build-request: foundry.gdd_doc absent, nothing to do';
    RETURN;
  END IF;

  CREATE TABLE IF NOT EXISTS foundry.build_request (
    id bigserial PRIMARY KEY,
    doc_id text NOT NULL REFERENCES foundry.gdd_doc(id),
    scene_slug text NOT NULL,
    requested_by_sid text NOT NULL,
    status text NOT NULL DEFAULT 'queued'
      CHECK (status IN ('queued','building','verifying','landed','failed')),
    detail text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
  );
  -- One live build per doc -- the duplicate refusal is race-safe at the index,
  -- mirroring registerScene's ON CONFLICT posture, not a losable pre-check.
  CREATE UNIQUE INDEX IF NOT EXISTS build_request_active_once
    ON foundry.build_request (doc_id)
    WHERE status IN ('queued','building','verifying');
  CREATE INDEX IF NOT EXISTS build_request_doc_idx
    ON foundry.build_request (doc_id, created_at DESC);
  CREATE INDEX IF NOT EXISTS build_request_status_idx
    ON foundry.build_request (status, created_at);
END
$mig$;
