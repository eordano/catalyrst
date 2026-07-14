/**
 * Data-freshness labels for governance pages.
 *
 * Everything catalyrst-governance serves on the read side is a mirror of an
 * upstream DAO API, populated by its sync loop. That loop is off by default
 * (`poll_enabled` defaults false -- catalyrst-governance/src/config.rs:133) and
 * `GOVERNANCE_POLL_ENABLED` is set nowhere in deploy/, nixos/ or
 * rig/, so on this deployment the mirror is frozen at the last manual
 * backfill/sync. A page that shows year-old rows with no date reads as current.
 *
 * These helpers only ever report what the payload itself says; they never
 * guess, and they return null rather than a plausible-looking date.
 */

/** Reason string used when a governance read cannot be served. */
export const GOVERNANCE_MIRROR_NOTE =
  "This node serves a mirror of the DAO. Its background sync is not enabled, so these records are only as fresh as the last manual sync.";

/** "1 Jan 2025", or null if the input is not a usable timestamp. */
export function governanceAsOfLabel(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return null;
  return d.toLocaleDateString("en-GB", {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  });
}

/**
 * Newest of the given timestamps, as an ISO string, or null when none parse.
 * Pass the raw record timestamps -- not "now".
 */
export function newestTimestamp(
  values: readonly (string | number | null | undefined)[],
): string | null {
  let best: number | null = null;
  for (const v of values) {
    if (v === null || v === undefined) continue;
    const ms = typeof v === "number" ? (v < 1e12 ? v * 1000 : v) : Date.parse(v);
    if (Number.isNaN(ms)) continue;
    if (best === null || ms > best) best = ms;
  }
  return best === null ? null : new Date(best).toISOString();
}
