// One half of the perf-parity gate: run every case in whichever mode this
// process resolved, and write the results where the runner can diff them.
//
// It captures rather than asserts because the comparison spans two processes.
// DCL_PERF is read by vite.validate.js at CONFIG time, so a single vitest run
// resolves exactly one set of aliases and can never hold both modes at once.
// scripts/check-perf-parity.mts runs this file twice and does the asserting.
//
// One process importing both the real module and its `.stub.ts` sibling would
// be cheaper and would prove less: the readers reach their schemas by specifier,
// so swapping one in-process means `vi.mock`, and a comparison run over a module
// graph no build produces answers a question nobody asked. Two runs over the
// same `validateAlias()` a build uses is the only version of this that binds.
//
// The mode is read off the ALIASES, never off the env var. An env var says what
// was requested; VALIDATION_ENABLED and a probe parse say what the module graph
// actually resolved to, which is the only thing that makes a comparison of two
// captures mean anything. A runner bug that failed to pass DCL_PERF through
// would otherwise diff a mode against itself and report perfect parity.

import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import process from "node:process";
import { test } from "vitest";

import { VALIDATION_ENABLED } from "../../../validate";
import { WearableSchema } from "../schemas/backpack";
import { CommunitySchema } from "../schemas/communities";
import { EventSchema } from "../schemas/events";
import { NotificationSchema } from "../schemas/notifications";
import { PlaceSchema } from "../schemas/places";
import { AvatarSchema } from "../schemas/profile";
import { ANCHORS, CASES } from "./cases";
import type { ParityCase } from "./cases";

/** Stands in for anything JSON has no honest representation of. */
function mark(what: string): string {
  return `<<${what}>>`;
}

/**
 * JSON that keeps the distinctions the diff exists to find.
 *
 * `JSON.stringify` erases the one most at stake here: a field normalized to
 * null and a field left undefined are both nothing to compare afterwards.
 * Objects keep their keys in sorted order, so key order cannot surface as a
 * false difference, and an explicitly-undefined value keeps its key.
 */
function encode(value: unknown, seen: Set<object> = new Set()): unknown {
  if (value === undefined) return mark("undefined");
  if (value === null) return null;
  if (typeof value === "bigint") return mark(`bigint ${value.toString()}`);
  if (typeof value === "function") return mark(`function ${value.name || "anonymous"}`);
  if (typeof value === "number") {
    if (Number.isNaN(value)) return mark("NaN");
    if (!Number.isFinite(value)) return mark(value > 0 ? "Infinity" : "-Infinity");
    if (Object.is(value, -0)) return mark("-0");
    return value;
  }
  if (typeof value !== "object") return value;

  const obj = value as object;
  if (seen.has(obj)) return mark("circular");
  seen.add(obj);
  try {
    if (Array.isArray(value)) return value.map((v) => encode(v, seen));
    if (value instanceof Date) return mark(`date ${value.toISOString()}`);
    if (value instanceof Map) {
      return { [mark("map")]: [...value].map(([k, v]) => [encode(k, seen), encode(v, seen)]) };
    }
    if (value instanceof Set) return { [mark("set")]: [...value].map((v) => encode(v, seen)) };

    const out: Record<string, unknown> = {};
    const proto: unknown = Object.getPrototypeOf(obj);
    const ctor = (obj as { constructor?: { name?: string } }).constructor?.name;
    // A reader handing back a class instance is itself a difference worth seeing.
    if (proto !== null && proto !== Object.prototype && ctor && ctor !== "Object") {
      out[mark("class")] = ctor;
    }
    for (const key of Object.keys(obj).sort()) {
      out[key] = encode((obj as Record<string, unknown>)[key], seen);
    }
    return out;
  } finally {
    seen.delete(obj);
  }
}

type CaptureEntry = {
  id: string;
  group: ParityCase["group"];
  probes: string[];
  note: string;
} & ({ outcome: "returned"; value: unknown } | { outcome: "threw"; error: string });

async function runCase(c: ParityCase): Promise<CaptureEntry> {
  const head = { id: c.id, group: c.group, probes: c.probes, note: c.note };
  try {
    return { ...head, outcome: "returned", value: encode(await c.run()) };
  } catch (err) {
    const e = err as { name?: string; message?: string };
    return {
      ...head,
      outcome: "threw",
      error: `${e?.name ?? "Error"}: ${e?.message ?? String(err)}`,
    };
  }
}

/**
 * Every aliased module, probed independently.
 *
 * Asking one schema whether the build is stubbed answers for that schema only:
 * a dead alias on any of the other five yields a green gate that compared perf
 * against perf for the module that broke. Confirmed by probe -- skipping `places`
 * in vite.validate.js's loop left the harness reporting perfect parity.
 *
 * The probe value is a STRING, not `{}`. An empty object looked like the obvious
 * discriminator and is not: a real schema whose fields are all optional accepts
 * it too, which reported the default build as "mixed" and failed the gate. No
 * object schema accepts a string, and the stub accepts everything, so this
 * separates them without depending on any schema's optionality.
 */
const ALIASED_PROBES: [string, { safeParse: (v: unknown) => { success: boolean } }][] = [
  ["communities", CommunitySchema],
  ["backpack", WearableSchema],
  ["events", EventSchema],
  ["notifications", NotificationSchema],
  ["places", PlaceSchema],
  ["profile", AvatarSchema],
];

test("capture catalyst reader output for this build mode", async () => {
  const stubbed = ALIASED_PROBES.filter(([, s]) => s.safeParse("not-an-object").success).map(
    ([n]) => n,
  );
  const schemasStubbed = stubbed.length === ALIASED_PROBES.length;
  const mode = stubbed.length === 0 ? "default" : schemasStubbed ? "perf" : "mixed";

  const cases: CaptureEntry[] = [];
  for (const c of CASES) cases.push(await runCase(c));

  // Anchors are evaluated here, not in the runner: `select` is a function and
  // cannot cross the JSON boundary. Only default mode is anchored -- perf is
  // allowed to check less, not to normalize less.
  const anchorFailures =
    mode === "default"
      ? ANCHORS.flatMap((a) => {
          const entry = cases.find((c) => c.id === a.id);
          if (!entry) {
            return [{ id: a.id, why: a.why, detail: `no case with id "${a.id}"` }];
          }
          if (entry.outcome !== "returned") {
            // A case that threw cannot be anchored, and silently skipping it
            // would let a crash read as a satisfied expectation.
            return [{ id: a.id, why: a.why, detail: `case threw: ${entry.error}` }];
          }
          let actual: unknown;
          try {
            // The captured value is ENCODED (undefined marked, keys sorted), so
            // an anchor's expectation is written against the encoded form.
            actual = a.select(entry.value);
          } catch (err) {
            return [{ id: a.id, why: a.why, detail: `select() threw: ${String(err)}` }];
          }
          const got = JSON.stringify(actual);
          const want = JSON.stringify(a.expect);
          return got === want
            ? []
            : [
                {
                  id: a.id,
                  why: a.why,
                  detail: `default mode produced ${got}\n      expected               ${want}`,
                },
              ];
        })
      : [];

  const capture = {
    mode,
    schemasStubbed,
    stubbedModules: stubbed,
    validationEnabled: VALIDATION_ENABLED,
    dclPerfEnv: process.env.DCL_PERF ?? "",
    caseCount: cases.length,
    anchorCount: mode === "default" ? ANCHORS.length : 0,
    anchorFailures,
    cases,
  };

  const out = process.env.DCL_PARITY_OUT ?? join(tmpdir(), `dcl-perf-parity-${mode}.json`);
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, `${JSON.stringify(capture, null, 2)}\n`);
  console.log(`perf-parity: captured ${cases.length} case(s) in ${mode} mode -> ${out}`);
});
