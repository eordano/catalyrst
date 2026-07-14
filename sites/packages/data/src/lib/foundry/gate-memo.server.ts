// The copilot door's verdict memo, keyed on the CANONICAL persona sid so every
// alias of one persona shares a single entry. It lives here rather than in the
// gate route because the writes that must invalidate it — a return-code redeem,
// an operator rebind — happen in this package, below the route layer.
//
// auth_request fires once per proxied request and the opencode UI loads dozens
// of assets plus an SSE stream, so verdicts are memoised for a short window. A
// role revoked mid-window keeps copilot access for at most TTL_MS — the actual
// mutations it guards re-check inside their own transactions.

const TTL_MS = 30_000;
const cache = new Map<string, { until: number; ok: boolean }>();

export function readGateMemo(canonSid: string): boolean | null {
  const hit = cache.get(canonSid);
  if (!hit || hit.until <= Date.now()) return null;
  return hit.ok;
}

export function writeGateMemo(canonSid: string, ok: boolean): void {
  if (cache.size > 10_000) cache.clear();
  cache.set(canonSid, { until: Date.now() + TTL_MS, ok });
}

/** Drops the memo for every given sid, so a rebind or a return-code redeem is
 *  honored at the door immediately instead of after the TTL. */
export function clearGateMemo(...sids: string[]): void {
  for (const sid of sids) cache.delete(sid);
}
