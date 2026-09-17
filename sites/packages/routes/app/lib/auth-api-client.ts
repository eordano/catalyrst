import { parseRecoverResponse, type LoadResult, type OutcomeError } from "./auth-request-recovery";

const AUTH_API = "/auth-api";
export const AUTH_API_TIMEOUT_MS = 10_000;
const AUTH_API_UNREACHABLE = "Couldn't reach the sign-in server.";

function authApiDeadline(): AbortSignal | undefined {
  return typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function"
    ? AbortSignal.timeout(AUTH_API_TIMEOUT_MS)
    : undefined;
}

export function withDeadline<T>(promise: Promise<T>, ms: number, onTimeout: T): Promise<T> {
  return new Promise<T>((resolve) => {
    const timer = setTimeout(() => resolve(onTimeout), ms);
    const settle = (value: T) => {
      clearTimeout(timer);
      resolve(value);
    };
    promise.then(settle, () => settle(onTimeout));
  });
}

export async function handoffWithDeadline<T>(
  post: (opts: { signal?: AbortSignal }) => Promise<T>,
): Promise<T> {
  try {
    return await post({ signal: authApiDeadline() });
  } catch (err) {
    const name = (err as Error)?.name;
    if (name === "TimeoutError" || name === "AbortError") throw new Error(AUTH_API_UNREACHABLE);
    throw err;
  }
}

export async function loadRequest(id: string): Promise<LoadResult> {
  let res: Response;
  try {
    res = await fetch(`${AUTH_API}/v2/requests/${encodeURIComponent(id)}`, {
      headers: { accept: "application/json" },
      cache: "no-store",
      signal: authApiDeadline(),
    });
  } catch {
    return { kind: "error", message: AUTH_API_UNREACHABLE };
  }
  if (res.ok) {
    const request = parseRecoverResponse(await res.json().catch(() => null));
    return request
      ? { kind: "ok", request }
      : { kind: "error", message: "The sign-in server returned an unexpected response." };
  }
  const body = (await res.json().catch(() => null)) as { error?: string } | null;
  const err = body?.error ?? "";
  if (/already been fulfilled|already has a response/.test(err)) return { kind: "fulfilled" };
  if (/has expired/.test(err)) return { kind: "expired" };
  if (/not found/.test(err)) return { kind: "not_found" };
  return { kind: "error", message: err || `The request couldn't be loaded (${res.status}).` };
}

export async function validationRequirement(id: string): Promise<boolean | null> {
  try {
    const res = await fetch(`${AUTH_API}/v2/requests/${encodeURIComponent(id)}/validation`, {
      cache: "no-store",
      signal: authApiDeadline(),
    });
    if (!res.ok) return null;
    const body = (await res.json()) as { requiresValidation?: boolean };
    return body?.requiresValidation === true;
  } catch {
    return null;
  }
}

export async function requiresValidation(id: string): Promise<boolean> {
  return (await validationRequirement(id)) !== false;
}

export async function postOutcome(
  id: string,
  body: { sender: string; result?: unknown; error?: OutcomeError },
): Promise<Response> {
  return fetch(`${AUTH_API}/v2/requests/${encodeURIComponent(id)}/outcome`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

export async function reportOutcome(
  id: string,
  body: { sender: string; result?: unknown; error?: OutcomeError },
): Promise<null> {
  return withDeadline(
    postOutcome(id, body).then(
      () => null,
      () => null,
    ),
    AUTH_API_TIMEOUT_MS,
    null,
  );
}
