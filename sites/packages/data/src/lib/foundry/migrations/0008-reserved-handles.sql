-- 0008-reserved-handles — the author handles of imported asks become reserved
-- persona names at import time, so a visitor cannot claim "JeyJey64" and stand
-- next to JeyJey64's quoted words. released_at is the operator's unblock for a
-- genuine returning author: a released row stays released across re-imports
-- (the importer inserts with ON CONFLICT DO NOTHING). Idempotent, guarded in
-- the 0003+ style; mirrored as end-state into schema.sql (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.request') IS NULL THEN
    RAISE NOTICE '0008-reserved-handles: foundry.request absent, nothing to do';
    RETURN;
  END IF;

  CREATE TABLE IF NOT EXISTS foundry.reserved_handle (
    handle            text PRIMARY KEY,
    source_request_id text REFERENCES foundry.request(id),
    reserved_at       timestamptz NOT NULL DEFAULT now(),
    released_at       timestamptz,
    release_note      text
  );
  CREATE UNIQUE INDEX IF NOT EXISTS reserved_handle_ci
    ON foundry.reserved_handle (lower(handle));
END
$mig$;
