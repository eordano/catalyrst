
export type UnavailableReason =
  | "not-configured"
  | "not-wired"
  | "unreachable"
  | "misrouted"
  | "not-connected"
  | "denied"
  | "backend-error";

export type Unavailable = {
  ok: false;
  reason: UnavailableReason;
  status: number;
  message: string;
  serverCheck: string | null;
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
