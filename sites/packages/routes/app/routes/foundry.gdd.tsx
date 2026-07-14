import { useEffect, useRef } from "react";
import { data } from "react-router";

import FdGddListPage from "@ui/foundry/pages/FdGddListPage";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/foundry/components/fdprovenancepill.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdgdd.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { publishCopilotDraft } from "@data/lib/foundry/gdd-publish.server";
import { listGddDocs, type GddListRow } from "@data/lib/foundry/gdd.server";
import { actionFailure, clientIp, requireCookie } from "../lib/foundry-action";

import type { Route } from "./+types/foundry.gdd";

export function meta() {
  return [{ title: "Design docs — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let docs: GddListRow[] = [];
  let unavailable = false;

  try {
    docs = await listGddDocs(getPool());
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({ badge: sidBadge(sid), unavailable, docs });
}

export async function action({ request }: Route.ActionArgs) {
  const base = sidLoader(request);
  const ip = clientIp(request);

  let form: FormData;
  try {
    form = await request.formData();
  } catch {
    return data(
      { ok: false, intent: "", error: "Could not read the form." },
      { status: 400 },
    );
  }
  const intent = String(form.get("intent") ?? "");
  if (intent !== "publish") {
    return data({ ok: false, intent, error: "Unknown action." }, { status: 400 });
  }

  try {
    requireCookie(base.created);
    const sessionId = String(form.get("sessionId") ?? "");
    const res = await publishCopilotDraft({ sessionId, sid: base.sid, ip });
    track(
      "fd_gdd_published",
      { doc_id: res.id, sections: res.sections },
      { sid: sidBadge(base.sid) },
    );
    return { ok: true, intent, error: null, id: res.id, title: res.title };
  } catch (err) {
    return actionFailure("foundry.gdd", intent, err);
  }
}

export default function FoundryGddList({ loaderData, actionData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("fd_gdd_list_viewed", { docs: d.docs.length }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  if (d.unavailable) {
    return (
      <div className="fd-page fd-stack fd-gdd">
        <FdPageHead title="Design docs" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  const publish = actionData
    ? {
        ok: actionData.ok === true,
        error: actionData.error ?? null,
        ...("id" in actionData && actionData.id ? { id: String(actionData.id) } : {}),
        ...("title" in actionData && actionData.title
          ? { title: String(actionData.title) }
          : {}),
      }
    : null;

  return <FdGddListPage docs={d.docs} publish={publish} />;
}
