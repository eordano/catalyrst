-- 0022-sid-alias — a persona outlives any one browser session. sid_alias maps
-- an abandoned session sid onto the persona's canonical sid so roles, consent,
-- and act attribution survive cookie loss. Append-only rows are never
-- rewritten; reads resolve through this table instead. An alias arrives two
-- ways only: a fresh sid that redeemed a return code ('return-code'), or an
-- operator moving a stranded grant ('operator-rebind'). Schema-only: no
-- existing row is touched.
-- Idempotent, guarded in the 0003+ style; schema.sql carries the same end
-- state (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.persona') IS NULL THEN
    RAISE NOTICE '0022-sid-alias: foundry.persona absent, nothing to do';
    RETURN;
  END IF;

  IF to_regclass('foundry.sid_alias') IS NOT NULL THEN
    RETURN;
  END IF;

  CREATE TABLE foundry.sid_alias (
    alias_sid   text PRIMARY KEY,
    persona_sid text NOT NULL REFERENCES foundry.persona(sid),
    via         text NOT NULL CHECK (via IN ('return-code', 'operator-rebind')),
    linked_at   timestamptz NOT NULL DEFAULT now(),
    CHECK (alias_sid <> persona_sid)
  );

  CREATE INDEX sid_alias_persona ON foundry.sid_alias (persona_sid);
END
$mig$;
