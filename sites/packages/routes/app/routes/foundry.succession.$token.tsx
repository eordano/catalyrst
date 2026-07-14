import { data, redirect, useFetcher } from "react-router";

import FdSuccessionPage, {
  type FdSuccessionView,
} from "@ui/foundry/pages/FdSuccessionPage";

import "@ui/atoms/button.css";
import "@ui/components/emptystate.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdsuccession.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { serializeSidCookie } from "@core/lib/experiments/assign";
import { track } from "@core/lib/telemetry/track";

import { FoundryUnavailableError, sidBadge } from "@data/lib/foundry/db.server";
import { actionFailure, clientIp, requireCookie } from "../lib/foundry-action";
import {
  acceptTransfer,
  getTransferForToken,
} from "@data/lib/foundry/continuity.server";

import type { Route } from "./+types/foundry.succession.$token";

export function meta() {
  return [{ title: "Stewardship transfer — The Foundry" }];
}

export async function loader({ params }: Route.LoaderArgs) {
  const token = params.token;
  let view: FdSuccessionView = { ok: false, reason: "unknown" };
  let unavailable = false;
  try {
    const t = await getTransferForToken(token);
    if (t === null) {
      view = { ok: false, reason: "unknown" };
    } else if (t.effectiveStatus === "offered") {
      view = {
        ok: true,
        offer: {
          sceneTitle: t.sceneTitle,
          from: t.from,
          note: t.note,
          expiresAt: t.expiresAt,
        },
      };
    } else {
      view = { ok: false, reason: t.effectiveStatus };
    }
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }
  return { view, unavailable };
}

export async function action({ params, request }: Route.ActionArgs) {
  const base = sidLoader(request);
  const sid = base.sid;
  const ip = clientIp(request);
  const token = params.token;

  try {
    requireCookie(base.created);
    const res = await acceptTransfer({ code: token, sid, ip });
    // The scene id is the play-route slug; the badge, never the raw sid.
    track("fd_transfer_accepted", { slug: res.sceneId }, { sid: sidBadge(sid) });
    return redirect(`/foundry/play/${res.sceneId}`);
  } catch (err) {
    return actionFailure("foundry.succession", "", err);
  }
}

export default function FoundrySuccession({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const fetcher = useFetcher<typeof action>();
  const pending = fetcher.state !== "idle";

  if (d.unavailable) {
    return null;
  }

  return (
    <FdSuccessionPage
      view={d.view}
      pending={pending}
      error={fetcher.data && !fetcher.data.ok ? fetcher.data.error : null}
      onAccept={() => fetcher.submit({ intent: "accept" }, { method: "post" })}
    />
  );
}
