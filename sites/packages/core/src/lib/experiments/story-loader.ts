import path from "node:path";

import { data } from "react-router";

import {
  ensureSid,
  expireSidCookie,
  hasSplitSidCookie,
  readVerifiedWallet,
  resolveAssignment,
  serializeSidCookie,
  sharedSidDomain,
  type Assignment,
} from "./assign";
import { parseStory } from "./context";
import { trackExposure } from "../telemetry/track";

export function parseVariantOverride(
  url: URL,
  experimentKey: string,
): string | undefined {
  const raw = url.searchParams.get("variant");
  if (!raw) return undefined;
  const sep = raw.indexOf(":");
  if (sep === -1) return undefined;
  const key = raw.slice(0, sep);
  const variant = raw.slice(sep + 1);
  if (key !== experimentKey || !variant) return undefined;
  return variant;
}

// One HTTP request mints one sid. React-router runs a layout loader and its leaf
// loader in parallel against the same request, so without this memo a cookieless
// visitor gets a different sid per loader: two Set-Cookie headers race for the
// browser, exposure is recorded against a sid the visitor never keeps, and their
// later events land in the other arm. Keyed on the request's AbortSignal, which
// is shared by every clone of one incoming request; the request object itself is
// the fallback.
const mintedSid = new WeakMap<object, { sid: string; created: boolean }>();

function requestKey(request: Request): object {
  const signal: unknown = request.signal;
  return signal && typeof signal === "object" ? (signal as object) : request;
}

function sidOnce(request: Request): { sid: string; created: boolean } {
  const key = requestKey(request);
  const memo = mintedSid.get(key);
  if (memo) return memo;
  const fresh = ensureSid(request);
  if (fresh.created) mintedSid.set(key, fresh);
  return fresh;
}

export function sidLoader(request: Request) {
  const { sid, created } = sidOnce(request);
  const wallet = readVerifiedWallet(request);
  const domain = sharedSidDomain(request);
  let headers: Headers | undefined;
  if (created) {
    // Under the shared parent the FIRST mint is already wide-scope, so one sid
    // serves every *.catalyst.example.com surface and no host-scope twin ever exists to
    // shadow it later.
    headers = new Headers({ "Set-Cookie": serializeSidCookie(sid, { domain }) });
  } else if (domain && hasSplitSidCookie(request)) {
    // A legacy narrow+wide pair: converge on this request's winner -- re-issued
    // wide -- and expire the host-scope twin, so the next request has exactly
    // one sid and attribution stops flipping with the browser's send order.
    headers = new Headers();
    headers.append("Set-Cookie", serializeSidCookie(sid, { domain }));
    headers.append("Set-Cookie", expireSidCookie());
  }
  return {
    sid,
    wallet,
    userKey: wallet ?? sid,
    created,
    wrap: <T,>(payload: T, init?: { status?: number }) =>
      data(payload, { status: init?.status, headers }),
  };
}

export async function storyLoader(
  request: Request,
  storyDir: string,
  fallback: Assignment,
  options?: { skipExposure?: boolean },
) {
  const base = sidLoader(request);
  let assignment = fallback;
  // A `?variant=` session is QA or tooling driving the surface, not a sample of
  // it: it never counts as an exposure, so a preview or a screen-tour capture
  // cannot move the readout it exists to inspect.
  let previewOverride = false;
  try {
    const story = parseStory(
      path.join(process.cwd(), "packages", "features", "src", "stories", storyDir),
    );
    assignment = await resolveAssignment(base.sid, story, { user: base.userKey });
    const override = parseVariantOverride(
      new URL(request.url),
      story.experiment.key,
    );
    if (override) {
      previewOverride = true;
      const v = story.experiment.variants.find((x) => x.id === override);
      if (v) {
        assignment = {
          variant: v.id,
          flags: v.flags,
          experimentKey: story.experiment.key,
        };
      }
    }
  } catch {
  }
  if (!options?.skipExposure && !previewOverride) {
    trackExposure({
      sid: base.sid,
      story: storyDir,
      variant: assignment.variant,
      experimentKey: assignment.experimentKey,
    });
  }
  return { ...base, assignment };
}
