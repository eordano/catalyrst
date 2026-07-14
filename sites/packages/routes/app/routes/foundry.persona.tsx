import { useEffect, useRef } from "react";
import { data, useFetcher } from "react-router";

import FdPersonaPage, {
  type FdPersonaFormValues,
  type FdPersonaVM,
} from "@ui/foundry/pages/FdPersonaPage";
import { fdAvatarSpec } from "@ui/foundry/components/FdPersonaChip";
import { BODY_SHAPE_URNS, type BodyId } from "@ui/data/randomIdentity";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/atoms/button.css";
import "@ui/explorer/components/avatarpreview.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdpersona.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { FoundryUnavailableError, sidBadge } from "@data/lib/foundry/db.server";
import {
  actionFailure,
  clientIp,
  reissueSidHeaders,
  requireCookie,
} from "../lib/foundry-action";
import {
  type CarryStatus,
  carryStatus,
  claimPersona,
  getPersona,
  hasActiveCarryCode,
  mintCarryCode,
  personaHistory,
  redeemCarryCode,
  validateDisplayName,
} from "@data/lib/foundry/persona.server";

import type { Route } from "./+types/foundry.persona";

export function meta() {
  return [{ title: "Your persona — The Foundry" }];
}

function isBody(value: string): value is BodyId {
  return value === "A" || value === "B";
}

function toIndex(value: string): number {
  const n = Number.parseInt(value, 10);
  return Number.isFinite(n) && n >= 0 ? n : 0;
}

export async function loader({ request }: Route.LoaderArgs) {
  const base = sidLoader(request);
  const sid = base.sid;
  const badge = sidBadge(sid);

  let persona: FdPersonaVM | null = null;
  let history: { at: string; name: string }[] = [];
  let carry: CarryStatus | null = null;
  let hasReturnCode = true;
  let unavailable = false;
  try {
    const row = await getPersona(sid);
    if (row) {
      persona = {
        displayName: row.displayName,
        avatar: fdAvatarSpec(row.avatar),
        words: row.words,
        claimedAt: row.claimedAt,
        updatedAt: row.updatedAt,
      };
      carry = await carryStatus(sid);
      hasReturnCode = await hasActiveCarryCode(sid);
    }
    const trail = await personaHistory(sid);
    history = trail.map((h) => ({
      at: h.at,
      name: typeof h.detail.display_name === "string" ? h.detail.display_name : "—",
    }));
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  // base.wrap carries the sidLoader's cookie work: a first mint (wide-scope on
  // *.catalyst.example.com) or a legacy split-jar convergence pair.
  return base.wrap({ badge, persona, history, carry, hasReturnCode, unavailable });
}

export async function action({ request }: Route.ActionArgs) {
  const base = sidLoader(request);
  const sid = base.sid;
  const ip = clientIp(request);

  let form: FormData;
  try {
    form = await request.formData();
  } catch {
    return data({ ok: false, error: "Could not read the form." }, { status: 400 });
  }

  const intent = String(form.get("intent") ?? "save");

  if (intent === "carry-mint") {
    try {
      requireCookie(base.created);
      const minted = await mintCarryCode(sid, ip);
      track("fd_carry_minted", { replaced: minted.replaced }, { sid: sidBadge(sid) });
      return {
        ok: true as const,
        intent: "carry-mint" as const,
        error: null,
        code: minted.code,
        replaced: minted.replaced,
      };
    } catch (err) {
      return actionFailure("foundry.persona", intent, err);
    }
  }

  if (intent === "carry-redeem") {
    try {
      requireCookie(base.created);
      const redeemed = await redeemCarryCode(sid, String(form.get("code") ?? ""), ip);
      track("fd_carry_redeemed", {}, { sid: sidBadge(redeemed.sid) });
      // The persona's own sid becomes this browser's session. On a *.catalyst.example.com
      // host the wide-scope cookie is the ONLY survivor: the host-scope twin
      // is expired in the same response, so no stale sid can shadow the new
      // one (reissueSidHeaders). The abandoned sid is already aliased onto
      // the persona by the redeem itself.
      return data(
        {
          ok: true as const,
          intent: "carry-redeem" as const,
          error: null,
          name: redeemed.displayName,
        },
        { headers: reissueSidHeaders(request, redeemed.sid) },
      );
    } catch (err) {
      return actionFailure("foundry.persona", intent, err);
    }
  }

  try {
    requireCookie(base.created);
    const displayName = String(form.get("displayName") ?? "").trim();
    // The same rules the DB enforces as CHECKs, surfaced as the form note's own
    // sentence instead of a constraint violation turned into a 500.
    const invalid = validateDisplayName(displayName);
    if (invalid) {
      return data({ ok: false, error: invalid }, { status: 400 });
    }
    const words = String(form.get("words") ?? "").trim();
    if (words.length > 280) {
      return data(
        { ok: false, error: "Keep your words to 280 characters." },
        { status: 400 },
      );
    }
    const bodyRaw = String(form.get("body") ?? "A");
    const body: BodyId = isBody(bodyRaw) ? bodyRaw : "A";
    const skin = toIndex(String(form.get("skin") ?? "0"));
    const hair = toIndex(String(form.get("hair") ?? "0"));
    const eyes = toIndex(String(form.get("eyes") ?? "0"));

    const claimed = await claimPersona({
      sid,
      displayName,
      avatarBodyUrn: BODY_SHAPE_URNS[body],
      avatar: { body, skin, hair, eyes },
      words,
      ip,
    });
    track("fd_persona_saved", { name_len: displayName.length }, { sid: sidBadge(sid) });
    // The one-time return code rides this response only — rendered once by the
    // page, never re-readable. The save also converges the cookie scope on
    // *.catalyst.example.com hosts so the persona's sid is the browser's single sid.
    return data(
      { ok: true as const, error: null, returnCode: claimed.returnCode },
      { headers: reissueSidHeaders(request, sid) },
    );
  } catch (err) {
    return actionFailure("foundry.persona", "", err);
  }
}

export default function FoundryPersona({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const fetcher = useFetcher<typeof action>();
  const pending = fetcher.state !== "idle";
  const mintFetcher = useFetcher<typeof action>();
  const redeemFetcher = useFetcher<typeof action>();

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current || d.unavailable) return;
    viewed.current = true;
    track("fd_persona_viewed", { claimed: d.persona !== null }, { sid: d.badge });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  // A successful redeem changed the session cookie — reload so every loader
  // re-reads under the persona's own sid.
  const redeemedOk = redeemFetcher.data?.ok === true;
  useEffect(() => {
    if (redeemedOk) window.location.replace("/foundry/persona");
  }, [redeemedOk]);

  const mintData = mintFetcher.data;
  const mintedCode =
    mintData && mintData.ok === true && "code" in mintData ? mintData.code : null;
  const mintedReplaced =
    mintData && mintData.ok === true && "replaced" in mintData
      ? mintData.replaced
      : false;

  if (d.unavailable) {
    return (
      <div className="fd-page fd-stack fd-persona">
        <FdPageHead title="Your persona" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  return (
    <FdPersonaPage
      persona={d.persona}
      badge={d.badge}
      history={d.history}
      error={fetcher.data && !fetcher.data.ok ? fetcher.data.error : null}
      saved={fetcher.data?.ok === true}
      pending={pending}
      carry={d.carry}
      carryCode={mintedCode}
      returnCode={
        fetcher.data?.ok === true && "returnCode" in fetcher.data
          ? fetcher.data.returnCode
          : null
      }
      hasReturnCode={d.hasReturnCode}
      carryReplaced={mintedReplaced}
      carryError={
        (mintFetcher.data && !mintFetcher.data.ok ? mintFetcher.data.error : null) ??
        (redeemFetcher.data && !redeemFetcher.data.ok ? redeemFetcher.data.error : null)
      }
      carryPending={mintFetcher.state !== "idle" || redeemFetcher.state !== "idle"}
      redeemedName={
        redeemFetcher.data?.ok === true && "name" in redeemFetcher.data
          ? redeemFetcher.data.name
          : null
      }
      onMintCarry={() => mintFetcher.submit({ intent: "carry-mint" }, { method: "post" })}
      onRedeemCarry={(code: string) =>
        redeemFetcher.submit({ intent: "carry-redeem", code }, { method: "post" })
      }
      onSubmit={(values: FdPersonaFormValues) =>
        fetcher.submit(
          {
            intent: "save",
            displayName: values.displayName,
            words: values.words,
            body: values.body,
            skin: String(values.skin),
            hair: String(values.hair),
            eyes: String(values.eyes),
          },
          { method: "post" },
        )
      }
    />
  );
}
