-- 0020-persona-carry-code — a persona lives in a session cookie and dies with
-- it, which contradicts the persistence pillar. A carry code is a one-time
-- bearer secret minted by the persona's holder; redeeming it in another
-- browser re-issues the persona's own sid cookie there, so every grant,
-- pledge and act follows automatically. Plaintext is shown once at mint and
-- never stored — only its sha256. Append-only: a newer mint supersedes the
-- last (revoked_at), redemption records the abandoned fresh sid.
-- Idempotent, guarded in the 0003+ style; schema.sql carries the same end
-- state (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.persona') IS NULL THEN
    RAISE NOTICE '0020-persona-carry-code: foundry.persona absent, nothing to do';
    RETURN;
  END IF;

  IF to_regclass('foundry.persona_carry_code') IS NOT NULL THEN
    RETURN;
  END IF;

  CREATE TABLE foundry.persona_carry_code (
    id            bigserial PRIMARY KEY,
    sid           text NOT NULL REFERENCES foundry.persona(sid),
    code_hash     text NOT NULL UNIQUE,
    created_at    timestamptz NOT NULL DEFAULT now(),
    revoked_at    timestamptz,
    redeemed_at   timestamptz,
    redeemed_from text,
    CONSTRAINT carry_code_redeem_pair
      CHECK ((redeemed_at IS NULL) = (redeemed_from IS NULL))
  );
  CREATE UNIQUE INDEX persona_carry_code_active_one
    ON foundry.persona_carry_code (sid)
    WHERE revoked_at IS NULL AND redeemed_at IS NULL;
END
$mig$;
