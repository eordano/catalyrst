import { useEffect, useRef, useState } from "react";
import { data, useFetcher } from "react-router";

import FdExchangePage from "@ui/foundry/pages/FdExchangePage";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/foundry/components/fdsection.css";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import type { StoryId } from "@core/lib/telemetry/story-id";

import {
  FoundryRateLimitError,
  FoundryUnavailableError,
  sidBadge,
} from "@data/lib/foundry/db.server";
import { actionFailure, clientIp, requireCookie } from "../lib/foundry-action";
import { activeRoles } from "@data/lib/foundry/roles.server";
import {
  createRequest,
  editRequest,
  listRequests,
  moderateRequest,
  pledgeRequest,
  withdrawPledge,
  type RequestBoardRow,
} from "@data/lib/foundry/exchange.server";

import type { Route } from "./+types/foundry.exchange";

const STORY: StoryId = "foundry/tour-activation";

const FALLBACK: Assignment = {
  variant: "control",
  flags: {},
  experimentKey: "foundry-tour-activation",
};

export function meta() {
  return [{ title: "Exchange — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(request, STORY, FALLBACK, {
    skipExposure: true,
  });

  // A design doc's "Take this design on" arrives with ?ask=<doc title> — a
  // visible draft the visitor edits, never an auto-submitted request.
  const draftTitle =
    new URL(request.url).searchParams.get("ask")?.slice(0, 80) ?? null;

  let requests: RequestBoardRow[] = [];
  let unavailable = false;
  let viewerIsAdmin = false;
  try {
    const [reqs, roles] = await Promise.all([listRequests(sid), activeRoles(sid)]);
    requests = reqs;
    viewerIsAdmin = roles.includes("admin");
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  const stats = {
    openRequests: requests.filter((r) => r.status === "open").length,
    totalPledges: requests.reduce((sum, r) => sum + r.pledges, 0),
  };

  return wrap({
    badge: sidBadge(sid),
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
    unavailable,
    draftTitle,
    requests,
    stats,
    viewerIsAdmin,
  });
}

// Foundry actions are open to any visitor by design: unlike the operator routes
// next door they carry no privileged token, touch nothing outside the program's
// own `foundry` schema, and attribute every write to the session that made it in
// `foundry.action_log`. The data layer caps writes per sid AND per client IP
// (the trusted x-real-ip below), so even a cookieless flood that rotates its sid
// still hits a ceiling; the worst a hostile visitor achieves is labeled graffiti
// inside a sandbox this program owns.
export async function action({ request }: Route.ActionArgs) {
  const { sid, created, assignment } = await storyLoader(request, STORY, FALLBACK, {
    skipExposure: true,
  });
  // Telemetry carries the 4-hex badge, never the raw sid: the badge indirection
  // exists precisely to keep the stable visitor identifier out of the store, and
  // it has to match what the browser fires so the funnel joins.
  const ctx = {
    sid: sidBadge(sid),
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  };
  const ip = clientIp(request);
  // A body we cannot parse as form data — e.g. a JSON payload posted to this
  // formData endpoint — throws here, before any handler runs. Answer with a
  // generic 400 rather than letting the parse error escape as a 500 carrying its
  // raw message.
  let form: FormData;
  try {
    form = await request.formData();
  } catch {
    return data(
      { ok: false, intent: "", errors: {}, error: "Could not read the form." },
      { status: 400 },
    );
  }
  const intent = String(form.get("intent") ?? "");

  try {
    requireCookie(created);
    switch (intent) {
      case "pledge": {
        const requestId = String(form.get("requestId") ?? "");
        const res = await pledgeRequest({ requestId, sid, ip });
        if (!res.alreadyPledged) {
          track(
            "fd_pledge_submitted",
            { pledges_after: res.pledges, request_id: requestId },
            ctx,
          );
        }
        return { ok: true, intent, errors: {}, error: null };
      }
      case "withdraw": {
        const requestId = String(form.get("requestId") ?? "");
        const res = await withdrawPledge({ requestId, sid, ip });
        if (res.deleted) {
          track(
            "fd_pledge_retracted",
            { pledges_after: res.pledges, request_id: requestId },
            ctx,
          );
        }
        return { ok: true, intent, errors: {}, error: null };
      }
      case "create": {
        const title = String(form.get("title") ?? "").trim();
        const body = String(form.get("body") ?? "").trim();
        const source = String(form.get("source") ?? "").trim();
        const errors: Record<string, string> = {};
        if (!title) errors.title = "Give the request a title.";
        else if (title.length > 80) errors.title = "Keep the title under 80 characters.";
        if (!body) errors.body = "Describe the play you wish existed.";
        else if (body.length > 280) errors.body = "Keep the description under 280 characters.";
        // An empty source is a first-person ask — the data layer records it as
        // its own provenance instead of making the asker invent a community.
        if (source.length > 60) errors.source = "Keep the source under 60 characters.";
        if (Object.keys(errors).length > 0) {
          return { ok: false, intent, errors, error: null };
        }
        const res = await createRequest({ title, body, source, sid, ip });
        track(
          "fd_request_submitted",
          { request_id: res.id, title_len: title.length },
          ctx,
        );
        return { ok: true, intent, errors: {}, error: null };
      }
      case "edit": {
        const requestId = String(form.get("requestId") ?? "");
        const title = String(form.get("title") ?? "").trim();
        const body = String(form.get("body") ?? "").trim();
        const errors: Record<string, string> = {};
        if (!title) errors.title = "Give the request a title.";
        else if (title.length > 80) errors.title = "Keep the title under 80 characters.";
        if (!body) errors.body = "Describe the play you wish existed.";
        else if (body.length > 280) errors.body = "Keep the description under 280 characters.";
        if (Object.keys(errors).length > 0) {
          return { ok: false, intent, errors, error: null };
        }
        await editRequest({ requestId, title, body, sid, ip });
        track(
          "fd_request_edited",
          { request_id: requestId, title_len: title.length },
          ctx,
        );
        return { ok: true, intent, errors: {}, error: null };
      }
      case "moderate": {
        const requestId = String(form.get("requestId") ?? "");
        const rawVerdict = String(form.get("verdict") ?? "");
        if (rawVerdict !== "approved" && rawVerdict !== "closed") {
          return { ok: false, intent, errors: {}, error: "Unknown verdict." };
        }
        await moderateRequest({ requestId, sid, verdict: rawVerdict, ip });
        track(
          "fd_request_moderated",
          { request_id: requestId, verdict: rawVerdict },
          ctx,
        );
        return { ok: true, intent, errors: {}, error: null };
      }
      default:
        return { ok: false, intent, errors: {}, error: "Unknown action." };
    }
  } catch (err) {
    // The shared limiter's own sentence names no wait. Its window is a minute
    // (db.server WINDOW_MS), so say that here — but only for the write-cap
    // message: requireCookie throws the same class with the cookie sentence,
    // which must survive untouched.
    if (
      err instanceof FoundryRateLimitError &&
      err.message.startsWith("Too many writes")
    ) {
      const message =
        intent === "create"
          ? "Too many writes from this session — wait a minute, then post again. Your draft is still in the form."
          : intent === "edit"
            ? "Too many writes from this session — wait a minute, then save again. Your edit is still in the form."
            : "Too many writes from this session — wait a minute, then try again.";
      return data(
        { ok: false, intent, errors: {}, error: message },
        { status: 429 },
      );
    }
    return actionFailure("foundry.exchange", intent, err, { errors: {} });
  }
}

export default function FoundryExchange({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = {
    sid: d.badge,
    story: STORY,
    variant: d.variant,
    experimentKey: d.experimentKey,
  };
  const fetcher = useFetcher<typeof action>();
  const [postOpen, setPostOpen] = useState(Boolean(d.draftTitle));
  const pending = fetcher.state !== "idle";

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track(
      "fd_exchange_viewed",
      { pledges_total: d.stats.totalPledges, requests_open: d.stats.openRequests },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  useEffect(() => {
    if (fetcher.state === "idle" && fetcher.data?.ok && fetcher.data.intent === "create") {
      setPostOpen(false);
    }
  }, [fetcher.state, fetcher.data]);

  if (d.unavailable) {
    return (
      <div className="fd-page fd-stack fd-exchange">
        <FdPageHead title="Exchange" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  const submit = (fields: Record<string, string>) =>
    fetcher.submit(fields, { method: "post" });

  return (
    <FdExchangePage
      requests={d.requests}
      postOpen={postOpen}
      draftTitle={d.draftTitle}
      onTogglePost={() => setPostOpen((v) => !v)}
      formErrors={fetcher.data?.errors ?? {}}
      error={fetcher.data?.error ?? null}
      pending={pending}
      viewerIsAdmin={d.viewerIsAdmin}
      onPledge={(requestId: string) => submit({ intent: "pledge", requestId })}
      onWithdraw={(requestId: string) => submit({ intent: "withdraw", requestId })}
      onModerate={(requestId: string, verdict: "approved" | "closed") =>
        submit({ intent: "moderate", requestId, verdict })
      }
      onPost={(values: { title: string; body: string; source: string }) =>
        submit({ intent: "create", ...values })
      }
    />
  );
}
