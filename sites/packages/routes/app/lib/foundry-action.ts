import { data } from "react-router";

import {
  expireSidCookie,
  serializeSidCookie,
  sharedSidDomain,
} from "@core/lib/experiments/assign";

import {
  FoundryRateLimitError,
  FoundryStateError,
  FoundryUnavailableError,
} from "@data/lib/foundry/db.server";

// Re-issues `sid` as the browser's ONE session cookie. Under the shared parent
// the wide-scope cookie is the sole survivor: the host-scope twin is expired in
// the same response, so two same-named different-scope sids can never coexist
// and the last-occurrence-wins cookie parser has nothing to mis-pick.
export function reissueSidHeaders(request: Request, sid: string): Headers {
  const headers = new Headers();
  const domain = sharedSidDomain(request);
  headers.append("Set-Cookie", serializeSidCookie(sid, { domain }));
  if (domain) headers.append("Set-Cookie", expireSidCookie());
  return headers;
}

// The client IP as our edge saw it, used as a second rate-limit bucket so a
// cookieless flood that rotates its sid still hits a ceiling. Trust x-real-ip:
// nginx sets it from $remote_addr — the socket peer, which the client cannot
// forge (config/nginx/conf.d/_proxy.inc). The X-Forwarded-For chain is
// client-controllable at the HEAD (a hostile client prepends any value it likes),
// so only its LAST element — the one our own nginx appended — is trustworthy; the
// first element never is.
export function clientIp(request: Request): string | null {
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

// A write that arrives with no sid cookie — so the loader minted a fresh one on
// this very request — is a cookieless client, which the per-session limit cannot
// bind. Refuse it rather than hand it an unlimited fresh budget.
export function requireCookie(created: boolean): void {
  if (created) {
    throw new FoundryRateLimitError(
      "This action needs a session cookie. Enable cookies for this site and try again.",
    );
  }
}

export function errorStatus(err: unknown): number {
  const status = (err as { status?: unknown } | null)?.status;
  return typeof status === "number" ? status : 500;
}

// Only errors that carry deliberate, user-facing copy are echoed back. A raw
// Postgres error (constraint names, columns, types) is logged and replaced
// with a generic sentence so schema detail never reaches an anonymous poster.
export function actionFailure<E extends Record<string, unknown> = Record<never, never>>(
  tag: string,
  intent: string,
  err: unknown,
  extra?: E,
) {
  let message: string;
  if (err instanceof FoundryUnavailableError) {
    message = "The program database is not configured.";
  } else if (
    err instanceof FoundryStateError ||
    err instanceof FoundryRateLimitError
  ) {
    message = err.message;
  } else {
    console.error(`${tag} action failed`, err);
    message = "That did not go through.";
  }
  const body = { ok: false, intent, ...extra, error: message } as {
    ok: boolean;
    intent: string;
    error: string;
  } & E;
  return data(body, { status: errorStatus(err) });
}
