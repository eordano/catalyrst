import { useEffect, useRef, useState } from "react";
import { data, useFetcher } from "react-router";

import EmptyState from "@ui/components/EmptyState";
import FdExchangePage from "@ui/foundry/pages/FdExchangePage";

import "@ui/components/emptystate.css";
import "@ui/foundry/pages/fdexchange.css";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import type { StoryId } from "@core/lib/telemetry/story-id";

import {
  FoundryRateLimitError,
  FoundryStateError,
  FoundryUnavailableError,
  sidBadge,
} from "@data/lib/foundry/db.server";
import {
  createRequest,
  listRequests,
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

// A refused write answers with its own status: 429 rate limit, 409 state conflict.
function errorStatus(err: unknown): number {
  const status = (err as { status?: unknown } | null)?.status;
  return typeof status === "number" ? status : 500;
}

// The client IP as our edge saw it, used as a second rate-limit bucket so a
// cookieless flood that rotates its sid still hits a ceiling. Trust x-real-ip:
// nginx sets it from $remote_addr — the socket peer, which the client cannot
// forge (config/nginx/conf.d/_proxy.inc). The X-Forwarded-For chain is
// client-controllable at the HEAD (a hostile client prepends any value it likes),
// so only its LAST element — the one our own nginx appended — is trustworthy; the
// first element never is.
function clientIp(request: Request): string | null {
  const real = request.headers.get("x-real-ip");
  if (real && real.trim() !== "") return real.trim();
  const xff = request.headers.get("x-forwarded-for");
  if (xff) {
    const parts = xff.split(",");
    const last = parts[parts.length - 1]?.trim();
    if (last) return last;
  }
  return null;
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(request, STORY, FALLBACK, {
    skipExposure: true,
  });

  let requests: RequestBoardRow[] = [];
  let unavailable = false;
  try {
    requests = await listRequests(sid);
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
    requests,
    stats,
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
    // A write that arrives with no sid cookie — so the loader minted a fresh one
    // on this very request — is a cookieless client, which the per-session limit
    // cannot bind. Refuse it rather than hand it an unlimited fresh budget.
    if (created) {
      throw new FoundryRateLimitError(
        "This action needs a session cookie. Enable cookies for this site and try again.",
      );
    }
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
      default:
        return { ok: false, intent, errors: {}, error: "Unknown action." };
    }
  } catch (err) {
    // Only errors that carry deliberate, user-facing copy are echoed back. A raw
    // Postgres error (constraint names, columns, types) is logged and replaced
    // with a generic sentence so schema detail never reaches an anonymous poster.
    let message: string;
    if (err instanceof FoundryUnavailableError) {
      message = "The program database is not configured.";
    } else if (
      err instanceof FoundryStateError ||
      err instanceof FoundryRateLimitError
    ) {
      message = err.message;
    } else {
      console.error("foundry.exchange action failed", err);
      message = "That did not go through.";
    }
    return data(
      { ok: false, intent, errors: {}, error: message },
      { status: errorStatus(err) },
    );
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
  const [postOpen, setPostOpen] = useState(false);
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
      <EmptyState
        variant="inline"
        title="Foundry database not configured"
        subtitle="Program state lives in Postgres. Set FOUNDRY_DATABASE_URL on this deployment to read and write it."
      />
    );
  }

  const submit = (fields: Record<string, string>) =>
    fetcher.submit(fields, { method: "post" });

  return (
    <FdExchangePage
      stats={d.stats}
      requests={d.requests}
      postOpen={postOpen}
      onTogglePost={() => setPostOpen((v) => !v)}
      formErrors={fetcher.data?.errors ?? {}}
      error={fetcher.data?.error ?? null}
      pending={pending}
      onPledge={(requestId: string) => submit({ intent: "pledge", requestId })}
      onWithdraw={(requestId: string) => submit({ intent: "withdraw", requestId })}
      onPost={(values: { title: string; body: string; source: string }) =>
        submit({ intent: "create", ...values })
      }
    />
  );
}
