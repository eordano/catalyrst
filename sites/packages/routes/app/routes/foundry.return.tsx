import { useEffect } from "react";
import { data, useFetcher } from "react-router";

import FdReturnPage from "@ui/foundry/pages/FdReturnPage";

import "@ui/atoms/button.css";
import "@ui/foundry/components/fdreturnform.css";
import "@ui/foundry/components/fdsection.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { sidBadge } from "@data/lib/foundry/db.server";
import { redeemCarryCode } from "@data/lib/foundry/persona.server";
import {
  actionFailure,
  clientIp,
  reissueSidHeaders,
  requireCookie,
} from "../lib/foundry-action";

import type { Route } from "./+types/foundry.return";

export function meta() {
  return [{ title: "Coming back? — The Foundry" }];
}

// The recovery door the persona page's own copy points to. The form also lives
// inline on /foundry/persona; this page exists so recovery never requires
// finding that page first.
export async function loader({ request }: Route.LoaderArgs) {
  const base = sidLoader(request);
  return base.wrap({ badge: sidBadge(base.sid) });
}

export async function action({ request }: Route.ActionArgs) {
  const base = sidLoader(request);
  const ip = clientIp(request);

  let form: FormData;
  try {
    form = await request.formData();
  } catch {
    return data({ ok: false, error: "Could not read the form." }, { status: 400 });
  }

  try {
    requireCookie(base.created);
    const redeemed = await redeemCarryCode(base.sid, String(form.get("code") ?? ""), ip);
    track("fd_carry_redeemed", {}, { sid: sidBadge(redeemed.sid) });
    // The persona's own sid becomes this browser's session, in exactly the
    // durable single-cookie state the claim path leaves behind: the wide-scope
    // cookie wins and the host-scope twin is expired in the same response.
    return data(
      { ok: true as const, error: null, name: redeemed.displayName },
      { headers: reissueSidHeaders(request, redeemed.sid) },
    );
  } catch (err) {
    return actionFailure("foundry.return", "carry-redeem", err);
  }
}

export default function FoundryReturn(_: Route.ComponentProps) {
  const fetcher = useFetcher<typeof action>();
  const ok = fetcher.data?.ok === true;

  // A successful redeem changed the session cookie — land on the persona page
  // so every loader re-reads under the persona's own sid.
  useEffect(() => {
    if (ok) window.location.replace("/foundry/persona");
  }, [ok]);

  return (
    <FdReturnPage
      pending={fetcher.state !== "idle"}
      error={fetcher.data && !fetcher.data.ok ? fetcher.data.error : null}
      redeemedName={
        fetcher.data?.ok === true && "name" in fetcher.data ? fetcher.data.name : null
      }
      onRedeem={(code) => fetcher.submit({ code }, { method: "post" })}
    />
  );
}
