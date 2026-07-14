import { useEffect, useRef } from "react";
import { data, useFetcher } from "react-router";

import FdPeoplePage, {
  type FdPersonRow,
  type FdRosterEntry,
} from "@ui/foundry/pages/FdPeoplePage";
import { fdAvatarSpec } from "@ui/foundry/components/FdPersonaChip";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/atoms/button.css";
import "@ui/explorer/components/avatarpreview.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdpeople.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import {
  FoundryStateError,
  FoundryUnavailableError,
  sidBadge,
} from "@data/lib/foundry/db.server";
import { actionFailure, clientIp, requireCookie } from "../lib/foundry-action";
import { listPersonas } from "@data/lib/foundry/persona.server";
import { listRoomPresence } from "@data/lib/foundry/room.server";
import {
  activeRoles,
  listRoster,
  mintInvite,
  redeemInvite,
} from "@data/lib/foundry/roles.server";

import type { Route } from "./+types/foundry.people";

export function meta() {
  return [{ title: "People — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const base = sidLoader(request);
  const sid = base.sid;

  try {
    const [directory, roster, myRoles, presence] = await Promise.all([
      listPersonas(),
      listRoster(),
      activeRoles(sid),
      listRoomPresence(),
    ]);
    const people: FdPersonRow[] = directory.map((p) => ({
      actor: { name: p.displayName, avatar: fdAvatarSpec(p.avatar) },
      roles: p.roles,
      claimedAt: p.claimedAt,
      requests: p.requests,
      requestIds: p.requestIds,
      words: p.words,
      pledges: p.pledges,
      pledgeRequestIds: p.pledgeRequestIds,
      lastSeen: p.lastSeen,
    }));
    const rosterRows: FdRosterEntry[] = roster.rows.map((r) => ({
      role: r.role,
      actor: r.actor,
      since: r.since,
    }));
    return base.wrap({
      unavailable: false,
      badge: sidBadge(sid),
      people,
      roster: { rows: rosterRows, notListed: roster.notListed },
      myRoles,
      presence,
    });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    return base.wrap({
      unavailable: true,
      badge: sidBadge(sid),
      people: [] as FdPersonRow[],
      roster: { rows: [] as FdRosterEntry[], notListed: 0 },
      myRoles: [] as string[],
      presence: null,
    });
  }
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
  const intent = String(form.get("intent") ?? "");

  try {
    requireCookie(base.created);
    if (intent === "redeem_invite") {
      const code = String(form.get("code") ?? "").trim();
      const consentSteward = String(form.get("consentSteward") ?? "") === "on";
      const { role, personaName } = await redeemInvite({
        code,
        sid,
        consentSteward,
        ip,
      });
      track("fd_invite_redeemed", { role }, { sid: sidBadge(sid) });
      return { ok: true, error: null, personaName };
    }
    if (intent === "mint_invite") {
      const role = String(form.get("role") ?? "").trim();
      const note = String(form.get("note") ?? "");
      const expiresRaw = String(form.get("expires") ?? "").trim();
      // The web mints the roles a host may hand out. An operator invite never
      // comes from a form: the bootstrap script is its only minter.
      if (role !== "host" && role !== "create" && role !== "start") {
        throw new FoundryStateError(
          "This form mints host, create and start invites only.",
        );
      }
      let expiresAt: string | null = null;
      if (expiresRaw !== "") {
        const t = Date.parse(expiresRaw);
        if (Number.isNaN(t)) {
          throw new FoundryStateError("The expiry is not a readable date.");
        }
        if (t <= Date.now()) {
          throw new FoundryStateError(
            "That expiry is already past — pick a future day, or leave it empty.",
          );
        }
        expiresAt = new Date(t).toISOString();
      }
      const { code } = await mintInvite({ sid, role, note, expiresAt, ip });
      track("fd_invite_minted", { role }, { sid: sidBadge(sid) });
      // The code travels only in this action's response — the ledger records
      // the mint (who, role, note), never the code.
      return { ok: true, error: null, minted: { code, role } };
    }
    return { ok: false, error: "Unknown action." };
  } catch (err) {
    return actionFailure("foundry.people", "", err);
  }
}

function isMintResult(
  r: unknown,
): r is { ok: boolean; error: string | null; minted: { code: string; role: string } } {
  return typeof r === "object" && r !== null && "minted" in r;
}

export default function FoundryPeople({ loaderData, actionData }: Route.ComponentProps) {
  const d = loaderData;
  const fetcher = useFetcher<typeof action>();
  const mintFetcher = useFetcher<typeof action>();
  const pending = fetcher.state !== "idle";
  // fetcher.data on a JS submit; actionData is the no-JS (native form) fallback.
  // A successful no-JS mint is recognisable by its `minted` payload; a no-JS
  // failure cannot say which form it came from and lands on the shared alert.
  const result =
    fetcher.data ?? (isMintResult(actionData) ? undefined : actionData);
  const mintResult =
    mintFetcher.data ?? (isMintResult(actionData) ? actionData : undefined);

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current || d.unavailable) return;
    viewed.current = true;
    track("fd_people_viewed", { listed: d.people.length }, { sid: d.badge });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  if (d.unavailable) {
    return (
      <div className="fd-page fd-stack fd-people">
        <FdPageHead title="People" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  return (
    <FdPeoplePage
      people={d.people}
      roster={d.roster}
      myRoles={d.myRoles}
      presence={d.presence}
      error={result && !result.ok ? result.error : null}
      redeemed={result?.ok === true}
      redeemedName={
        result?.ok === true && "personaName" in result ? result.personaName : null
      }
      pending={pending}
      onRedeem={({ code, consentSteward }) =>
        fetcher.submit(
          {
            intent: "redeem_invite",
            code,
            ...(consentSteward ? { consentSteward: "on" } : {}),
          },
          { method: "post" },
        )
      }
      minted={
        isMintResult(mintResult) && mintResult.ok === true
          ? mintResult.minted
          : null
      }
      mintError={mintResult && !mintResult.ok ? mintResult.error : null}
      mintPending={mintFetcher.state !== "idle"}
      onMint={({ role, note, expires }) =>
        mintFetcher.submit(
          { intent: "mint_invite", role, note, expires },
          { method: "post" },
        )
      }
    />
  );
}
