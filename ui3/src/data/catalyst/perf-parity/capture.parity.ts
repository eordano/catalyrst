
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

function mark(what: string): string {
  return `<<${what}>>`;
}

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

  const anchorFailures =
    mode === "default"
      ? ANCHORS.flatMap((a) => {
          const entry = cases.find((c) => c.id === a.id);
          if (!entry) {
            return [{ id: a.id, why: a.why, detail: `no case with id "${a.id}"` }];
          }
          if (entry.outcome !== "returned") {
            return [{ id: a.id, why: a.why, detail: `case threw: ${entry.error}` }];
          }
          let actual: unknown;
          try {
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
