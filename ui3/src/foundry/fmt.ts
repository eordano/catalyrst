// Deterministic across the server render and the client hydration: UTC fields
// formatted by hand (Intl's output moves with the runtime's ICU build), or
// digits parsed straight out of the ISO string itself.

export const MONTHS = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun",
  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/** "Aug 6, 2026 · 04:58 UTC" — the one instant format. */
export function stampUTC(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(iso);
  if (!m) return iso;
  const [, y, mo, d, hh, mm] = m;
  return `${MONTHS[Number(mo) - 1]} ${Number(d)}, ${y} · ${hh}:${mm} UTC`;
}

/** "Aug 6, 2026"; null in → null out. */
export function dayUTC(iso: string | null): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return `${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}, ${d.getUTCFullYear()}`;
}

/** "1 run", "2 runs". Every rendered count goes through this. */
export function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

/** Grouped by hand: Intl's separators move with the runtime's ICU build, and
 *  these strings have to match between the server render and hydration. */
export function groupDigits(n: number): string {
  const digits = String(Math.trunc(Math.abs(n)));
  let out = "";
  for (let i = 0; i < digits.length; i += 1) {
    if (i > 0 && (digits.length - i) % 3 === 0) out += ",";
    out += digits[i];
  }
  return n < 0 ? `-${out}` : out;
}

/** Display name for a stored role. The internal id stays 'admin' so recorded
 *  grants and consents keep working; a human reads "operator". */
export function roleLabel(role: string): string {
  return role === "admin" ? "operator" : role;
}
