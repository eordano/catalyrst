import { appendFileSync } from "node:fs";

import { describe, it } from "vitest";

/**
 * TypeScript half of the anti-silent-skip contract. Same opt-out variable, same
 * skiplog and byte-identical SKIPPED marker as catalyrst-testgate (Rust) and
 * rig/lib/testgate.sh, so scripts/no-silent-skips.sh polices all three with one
 * grep. A suite whose dependency is missing FAILS; only the deliberate opt-out
 * downgrades that to a skip, and the skip is loud enough for the gate to see.
 */
export const OPT_OUT = "ALLOW_SKIPPED_INTEGRATION";
export const SKIP_LOG = "CATALYRST_TESTGATE_SKIPLOG";

export function skipsAllowed(): boolean {
  const raw = process.env[OPT_OUT];
  if (raw === undefined || raw === "") return false;
  return raw !== "0" && raw.toLowerCase() !== "false";
}

export function refusal(requirement: string, detail: string): string {
  return [
    `integration dependency unavailable: ${requirement}`,
    `  ${detail}`,
    "  this test asserts nothing without it, so it fails instead of passing.",
    `  provide ${requirement}, or set ${OPT_OUT}=1 to let it skip on a machine that cannot run it.`,
  ].join("\n");
}

export function marker(
  name: string,
  requirement: string,
  detail: string,
): string {
  return `SKIPPED ${name}: ${requirement} unavailable (${detail}); ${OPT_OUT} is set`;
}

function recordSkip(name: string, requirement: string, detail: string): void {
  // eslint-disable-next-line no-console
  console.error(marker(name, requirement, detail));
  const path = process.env[SKIP_LOG];
  if (!path) return;
  try {
    appendFileSync(path, `${name}\t${requirement}\t${detail}\n`, "utf8");
  } catch {
    // a missing skiplog must not mask the skip itself
  }
}

/**
 * `describe` when the dependency is present; a suite that fails when it is not;
 * `describe.skip` plus the marker when the opt-out is deliberately set.
 */
export function describeRequiring(
  present: boolean,
  requirement: string,
  detail: string,
): typeof describe {
  if (present) return describe;

  if (skipsAllowed()) {
    const failing = (name: string, body?: unknown): void => {
      recordSkip(name, requirement, detail);
      (describe.skip as unknown as (n: string, b?: unknown) => void)(name, body);
    };
    return failing as unknown as typeof describe;
  }

  const failing = (name: string, _body?: unknown): void => {
    describe(name, () => {
      it(`cannot run: ${requirement} is unavailable`, () => {
        throw new Error(refusal(requirement, detail));
      });
    });
  };
  return failing as unknown as typeof describe;
}

export const PG_REQUIREMENT =
  "a PostgreSQL server for the sites e2e suite (SITES_E2E_PG_URL, or pg_tmp/initdb on PATH)";

export function describeRequiringPg(): typeof describe {
  return describeRequiring(
    process.env.SITES_E2E_PG === "ready",
    PG_REQUIREMENT,
    "globalSetup found no usable Postgres; run inside `nix develop ./catalyrst/sites`, " +
      "or point SITES_E2E_PG_URL at a server it may create a scratch schema in",
  );
}
