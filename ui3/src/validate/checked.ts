// The checking implementation of `check`. `validate/index.ts` imports it
// through the bare id `dcl-validate-impl`, which the build aliases here or to
// `unchecked.ts`; see that file for why the indirection exists at all.

import type { ZodType } from "zod";

export const VALIDATION_ENABLED = true;

/** Reported once per (boundary, message) so a bad frame arriving 60x a second
 *  cannot flood the console into uselessness. */
const reported = new Set<string>();

/** Counts every rejection, so a UI or a test can ask whether drift happened
 *  rather than scraping console output. */
const failures = new Map<string, number>();

export function validationFailures(): ReadonlyMap<string, number> {
  return failures;
}

export function resetValidationFailures(): void {
  failures.clear();
  reported.clear();
  reporter = null;
  devOverride = null;
}

/** What a rejection is handed to besides the console. */
export type ValidationReporter = (report: {
  boundary: string;
  detail: string;
  /** Dotted paths only -- never the payload. See setValidationReporter. */
  paths: string[];
}) => void;

let reporter: ValidationReporter | null = null;

/**
 * Route rejections somewhere durable. Unset, a production rejection is a
 * console line in a browser nobody is watching.
 *
 * Injected rather than imported because the transport is `track()` in
 * sites/packages/core, and the dependency runs sites -> ui3 only: importing it
 * here would invert that. sites installs the telemetry reporter at startup;
 * ui3 standalone keeps the console.
 *
 * Reports carry the boundary, the issue text and the failing PATHS -- never
 * values. A rejected payload is exactly the kind of thing most likely to hold a
 * wallet address or a chat line, and drift is diagnosable from the shape alone.
 */
export function setValidationReporter(next: ValidationReporter | null): void {
  reporter = next;
}

// Vite REPLACES import.meta.env.DEV with a boolean literal at transform time,
// which is what makes the dev throw free in a production bundle -- and also
// means this cannot be read or reassigned at runtime. Without an override the
// production branch of `check` is unreachable from a test, and it stayed
// untested for exactly that reason.
//
// The override is test-only surface in shipped code, which is a cost worth
// paying here: the alternative is an untested failure path in the one function
// every boundary depends on. `resetValidationFailures` clears it, so a test
// cannot leak the override into the next one.
const INLINED_DEV = (() => {
  try {
    return Boolean((import.meta as { env?: { DEV?: boolean } }).env?.DEV);
  } catch {
    return false;
  }
})();

let devOverride: boolean | null = null;

/** Test hook: force the dev (throwing) or production (logging) branch. */
export function setValidationDevMode(dev: boolean | null): void {
  devOverride = dev;
}

function isDev(): boolean {
  return devOverride ?? INLINED_DEV;
}

/**
 * A dotted path with identifying segments removed.
 *
 * "Paths, never values" is not sufficient on its own, and assuming it was is
 * how wallet addresses reached a server. Several persisted stores are
 * `z.record(z.string(), ...)` keyed by the signer, so the KEY is the identifier
 * and it lands in the path: `0x1d9f...e11.txHash`. From there `detail` reaches
 * console.warn, which sites patches globally and POSTs to /internal/client-error.
 *
 * The shape is what makes a report actionable, and the shape survives: an
 * address, a long hex blob or a uuid becomes `<key>`, so `<key>.txHash` still
 * says exactly which field of which store drifted while the identifier never
 * forms. Numeric array indices are kept -- they are structure, not identity.
 */
function redactPath(path: readonly PropertyKey[]): string {
  return path
    .map((seg) => {
      if (typeof seg !== "string") return String(seg);
      const identifying =
        /^0x[0-9a-fA-F]{6,}$/.test(seg) ||
        /^[0-9a-fA-F]{16,}$/.test(seg) ||
        /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-/.test(seg);
      return identifying ? "<key>" : seg;
    })
    .join(".");
}

function evaluate<T>(
  schema: ZodType<T>,
  value: unknown,
  boundary: string,
): { ok: boolean; data: T } {
  const result = schema.safeParse(value);
  if (result.success) return { ok: true, data: result.data };

  failures.set(boundary, (failures.get(boundary) ?? 0) + 1);

  const paths = result.error.issues.map((i) => redactPath(i.path));
  const detail = result.error.issues
    .slice(0, 3)
    .map((i, n) => `${paths[n] || "(root)"}: ${i.message}`)
    .join("; ");

  const first = !reported.has(boundary);
  if (first) reported.add(boundary);

  // Behind the once-per-boundary gate, not in front of it. The reporter sites
  // installs does its own console.warn AND an unbatched POST per call, and the
  // engine pushes at 10 Hz -- so a single drifted field used to mean ten reports
  // and ten requests every second, for as long as the drift lasted. The counter
  // above still increments every time, so nothing is lost from
  // validationFailures(); only the reporting is deduplicated.
  if (first && reporter) {
    try {
      reporter({ boundary, detail, paths });
    } catch {
      // A broken reporter must not become a broken app.
    }
  }

  if (isDev()) {
    throw new Error(
      `validation failed at ${boundary} \u{2014} ${detail}\n` +
        "If this is persisted state after a branch switch, the stored value was written by " +
        "an older build: clear it from localStorage and reload.",
    );
  }
  if (first) {
    console.warn(`[validate] ${boundary} \u{2014} ${detail} (further reports suppressed)`);
  }
  return { ok: false, data: value as T };
}

/**
 * Validate `value` against `schema` at a named boundary.
 *
 * Dev throws and production logs, which is a deliberate asymmetry: a developer
 * should not be able to ignore a shape that does not match, and a user should
 * not lose a session over one. The `boundary` string is what ties a production
 * telemetry report back to the dev failure -- keep it stable and specific.
 *
 * On a production rejection the ORIGINAL value is returned, not a default.
 * Callers here already have a fallback for missing data, and substituting a
 * plausible-looking default would hide the drift this exists to expose. Use
 * `checkOk` instead where carrying on with a bad value is itself unsafe.
 */
export function check<T>(schema: ZodType<T>, value: unknown, boundary: string): T {
  return evaluate(schema, value, boundary).data;
}

/**
 * Validate and report exactly as `check` does, but answer whether it held --
 * for callers that must SKIP a bad payload rather than carry on with it.
 *
 * `check` suits a caller whose next step tolerates a missing field. The engine
 * bridge is not one: its reducer reads `push.address.slice(...)`, so a
 * wrong-TYPED field is a TypeError one line after the rejection was logged.
 * The `?? prev.x` fallbacks only ever guarded ABSENT fields. Dev still throws.
 */
export function checkOk(schema: ZodType<unknown>, value: unknown, boundary: string): boolean {
  return evaluate(schema, value, boundary).ok;
}
