/**
 * Honest availability results for admin / operator controls.
 *
 * Every privileged surface in this directory answers with one of these instead
 * of throwing, returning a fixture, or returning an empty list that reads as
 * "there is nothing here". The distinction matters: an empty table and a table
 * you are not allowed to see look identical to a moderator, and the second one
 * must say so.
 *
 * `serverCheck` is the file:line of the server-side authorization that was
 * read before this control was allowed to exist. A control with
 * `serverCheck: null` has no server-side gate at all and must never be wired
 * to a write path.
 */

export type UnavailableReason =
  /** A real server-side gate exists, but this node has no credential for it. */
  | "not-configured"
  /** No server-side check exists, or no endpoint exists at all. */
  | "not-wired"
  /** The endpoint exists and is gated, but is not reachable from the edge. */
  | "unreachable"
  /** The client call is misrouted or mis-signed; a named fix is pending. */
  | "misrouted"
  /** The caller has no session / connected wallet. */
  | "not-connected"
  /** The server answered, and the answer was no. */
  | "denied"
  /** The backend could not be reached, or answered unusably. */
  | "backend-error";

export type Unavailable = {
  ok: false;
  reason: UnavailableReason;
  /** HTTP-ish status for the UI to key off. 0 when no request was made. */
  status: number;
  /** One line, safe to render verbatim to an operator. */
  message: string;
  /** file:line of the server-side check, or null when none exists. */
  serverCheck: string | null;
  /** The named change that would make this available, when there is one. */
  fix?: string;
};

export type Available<T> = { ok: true; data: T };

export type ControlResult<T> = Available<T> | Unavailable;

export function available<T>(data: T): Available<T> {
  return { ok: true, data };
}

export function unavailable(
  reason: UnavailableReason,
  message: string,
  opts: { status?: number; serverCheck?: string | null; fix?: string } = {},
): Unavailable {
  return {
    ok: false,
    reason,
    status: opts.status ?? 0,
    message,
    serverCheck: opts.serverCheck ?? null,
    ...(opts.fix ? { fix: opts.fix } : {}),
  };
}

/**
 * Maps a backend status onto an unavailable result. Used only after a request
 * was actually made and the server answered.
 */
export function unavailableFromStatus(
  status: number,
  message: string,
  serverCheck: string | null,
): Unavailable {
  const reason: UnavailableReason =
    status === 401 || status === 403
      ? "denied"
      : status === 503
        ? "not-configured"
        : "backend-error";
  return { ok: false, reason, status, message, serverCheck };
}
