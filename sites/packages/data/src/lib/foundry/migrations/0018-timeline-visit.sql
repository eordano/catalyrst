-- 0018-timeline-visit — one row per sid: when they last opened the timeline.
-- The "since your last visit" line needs a real visit marker; deriving it from
-- action_log would conflate acting with visiting (a browser that only reads
-- never appears there). Idempotent, guarded in the 0003+ style; schema.sql
-- carries the same end state (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.timeline_visit') IS NOT NULL THEN
    RETURN;
  END IF;

  CREATE TABLE foundry.timeline_visit (
    sid text PRIMARY KEY,
    at  timestamptz NOT NULL
  );
END
$mig$;
