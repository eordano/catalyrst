-- 0021-gdd-approval — the deck's hardest AI boundary made recordable: a
-- person signs a specific doc version. Versions are immutable (an edit mints
-- v(n+1)), so a signature can never be hollowed out by later edits. The
-- signer is a claimed persona (a signature needs a name); the row is
-- append-only and one per (doc, signer). No approval row = "no person has
-- approved this version", rendered exactly so.
-- Idempotent, guarded in the 0003+ style; schema.sql carries the same end
-- state (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.gdd_doc') IS NULL THEN
    RAISE NOTICE '0021-gdd-approval: foundry.gdd_doc absent, nothing to do';
    RETURN;
  END IF;

  IF to_regclass('foundry.gdd_approval') IS NOT NULL THEN
    RETURN;
  END IF;

  CREATE TABLE foundry.gdd_approval (
    id     bigserial PRIMARY KEY,
    doc_id text NOT NULL REFERENCES foundry.gdd_doc(id),
    sid    text NOT NULL,
    at     timestamptz NOT NULL DEFAULT now()
  );
  CREATE UNIQUE INDEX gdd_approval_once
    ON foundry.gdd_approval (doc_id, sid);
END
$mig$;
