import { data, useFetcher } from "react-router";

import FdSessionsPage, {
  type FdSessionOccurrenceVM,
} from "@ui/foundry/pages/FdSessionsPage";

import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/atoms/button.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdstat.css";
import "@ui/foundry/pages/fdsessions.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { FoundryUnavailableError, sidBadge } from "@data/lib/foundry/db.server";
import { actionFailure, clientIp, requireCookie } from "../lib/foundry-action";
import { activeRoles } from "@data/lib/foundry/roles.server";
import {
  createSeries,
  listUpcoming,
  retireSeries,
  rsvp,
  validateSeriesInput,
  withdrawRsvp,
} from "@data/lib/foundry/sessions.server";
import type { SessionSeriesInput } from "@data/lib/foundry/types";
import { useEffect, useRef, useState } from "react";

import type { Route } from "./+types/foundry.sessions";

export function meta() {
  return [{ title: "Sessions — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const base = sidLoader(request);
  const sid = base.sid;

  try {
    const [occurrences, myRoles] = await Promise.all([
      listUpcoming(sid),
      activeRoles(sid),
    ]);
    const rows: FdSessionOccurrenceVM[] = occurrences.map((o) => ({
      seriesId: o.seriesId,
      title: o.title,
      body: o.body,
      sceneId: o.sceneId,
      sceneTitle: o.sceneTitle,
      cadence: o.cadence,
      occurrenceAt: o.occurrenceAt,
      durationMinutes: o.durationMinutes,
      host: o.host,
      rsvpCount: o.rsvpCount,
      viewerRsvped: o.viewerRsvped,
      label: o.label,
    }));
    return base.wrap({
      unavailable: false,
      badge: sidBadge(sid),
      occurrences: rows,
      canHost: myRoles.includes("host"),
    });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    return base.wrap({
      unavailable: true,
      badge: sidBadge(sid),
      occurrences: [] as FdSessionOccurrenceVM[],
      canHost: false,
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
    return data(
      { ok: false, intent: "", error: "Could not read the form." },
      { status: 400 },
    );
  }
  const intent = String(form.get("intent") ?? "");

  try {
    requireCookie(base.created);
    const ctx = { sid: sidBadge(sid) };
    switch (intent) {
      case "rsvp": {
        const seriesId = String(form.get("seriesId") ?? "");
        const occurrenceAt = String(form.get("occurrenceAt") ?? "");
        const res = await rsvp({ seriesId, occurrenceAt, sid, ip });
        if (res.added) {
          track("fd_session_rsvped", { series_id: seriesId }, ctx);
        }
        return { ok: true, intent, error: null };
      }
      case "withdraw": {
        const seriesId = String(form.get("seriesId") ?? "");
        const occurrenceAt = String(form.get("occurrenceAt") ?? "");
        const res = await withdrawRsvp({ seriesId, occurrenceAt, sid, ip });
        if (res.deleted) {
          track("fd_session_rsvp_withdrawn", { series_id: seriesId }, ctx);
        }
        return { ok: true, intent, error: null };
      }
      case "create": {
        const durationMinutes = Number.parseInt(
          String(form.get("durationMinutes") ?? ""),
          10,
        );
        if (!Number.isFinite(durationMinutes)) {
          return data(
            { ok: false, intent, error: "Duration must be between 15 and 480 minutes." },
            { status: 400 },
          );
        }
        const input: SessionSeriesInput = {
          title: String(form.get("title") ?? "").trim(),
          body: String(form.get("body") ?? "").trim(),
          sceneId: String(form.get("sceneId") ?? "").trim() || null,
          cadence: String(form.get("cadence") ?? "") === "once" ? "once" : "weekly",
          firstAt: String(form.get("firstAt") ?? ""),
          durationMinutes,
        };
        const invalid = validateSeriesInput(input);
        if (invalid) {
          return data({ ok: false, intent, error: invalid }, { status: 400 });
        }
        const { id } = await createSeries({ sid, input, ip });
        track("fd_session_created", { cadence: input.cadence, series_id: id }, ctx);
        return { ok: true, intent, error: null };
      }
      case "retire": {
        const seriesId = String(form.get("seriesId") ?? "");
        await retireSeries({ sid, seriesId, ip });
        track("fd_session_retired", { series_id: seriesId }, ctx);
        return { ok: true, intent, error: null };
      }
      default:
        return { ok: false, intent, error: "Unknown action." };
    }
  } catch (err) {
    return actionFailure("foundry.sessions", intent, err);
  }
}

export default function FoundrySessions({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const fetcher = useFetcher<typeof action>();
  const pending = fetcher.state !== "idle";
  const [createOpen, setCreateOpen] = useState(false);

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current || d.unavailable) return;
    viewed.current = true;
    track("fd_sessions_viewed", { upcoming: d.occurrences.length }, { sid: d.badge });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  if (d.unavailable) {
    return (
      <div className="fd-page fd-stack fd-sessions">
        <FdPageHead title="The community calendar" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  const submit = (fields: Record<string, string>) =>
    fetcher.submit(fields, { method: "post" });

  return (
    <FdSessionsPage
      occurrences={d.occurrences}
      canHost={d.canHost}
      createOpen={createOpen}
      onToggleCreate={() => setCreateOpen((v) => !v)}
      error={fetcher.data && !fetcher.data.ok ? fetcher.data.error : null}
      pending={pending}
      onRsvp={({ seriesId, occurrenceAt }) =>
        submit({ intent: "rsvp", seriesId, occurrenceAt })
      }
      onWithdraw={({ seriesId, occurrenceAt }) =>
        submit({ intent: "withdraw", seriesId, occurrenceAt })
      }
      onCreate={(values) => {
        submit({ intent: "create", ...values });
        setCreateOpen(false);
      }}
      onRetire={(seriesId) => submit({ intent: "retire", seriesId })}
    />
  );
}
