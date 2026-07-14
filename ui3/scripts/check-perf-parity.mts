// Prove that performance mode strips VALIDATION and nothing else.
//
// DCL_PERF=1 aliases the six catalyst schema modules to always-accepting stubs.
// The intent is to remove checking; what actually leaves with it is everything
// else those modules carry -- a CDN rewrite, every `nullish -> null`, an emote
// category reshape -- because a stub replaces the whole module, transforms
// included. That is a behaviour change shipped under the name of an
// optimization, and it is invisible from either build alone.
//
// So both are run and their outputs compared:
//
//   parity cases      every row is valid, so validation has nothing to reject.
//                     The two outputs must be IDENTICAL. Any difference is a
//                     normalization the perf build lost.
//   guarded cases     one row is malformed in a way the reader's own structural
//                     guard rejects (src/data/catalyst/rows.ts). A guard is
//                     zod-free and lives beside its mapper, so the stub cannot
//                     take it: both modes drop the row and the two outputs must
//                     be IDENTICAL. This is where "the guard survives perf mode"
//                     stops being a claim about source code.
//   robustness cases  one row is malformed in a way only the SCHEMA rejects.
//                     Perf keeping it is allowed -- that difference IS "what is
//                     checked". Losing the valid rows is not: a stub that hands
//                     a bad row to a view mapper makes the mapper throw, and the
//                     read returns nothing at all.
//
// Two processes, because vite.validate.js reads DCL_PERF at config time: one
// vitest run resolves one set of aliases. Each run captures to a temp file and
// records the mode it OBSERVED (a probe parse against a real schema), so a
// runner that failed to pass the env var through is caught here rather than
// reported as perfect parity.
//
//   node scripts/check-perf-parity.mts [--json <dir>]

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const UI3 = fileURLToPath(new URL("..", import.meta.url));
const CONFIG = "vitest.perf-parity.config.ts";

const keepAt = (() => {
  const i = process.argv.indexOf("--json");
  return i >= 0 ? process.argv[i + 1] : null;
})();

const outDir = keepAt ?? mkdtempSync(join(tmpdir(), "dcl-perf-parity-"));

function capture(mode) {
  const file = join(outDir, `${mode}.json`);
  const env = { ...process.env, DCL_PARITY_OUT: file, DCL_PERF: mode === "perf" ? "1" : "" };
  try {
    execFileSync("npx", ["vitest", "run", "--config", CONFIG], {
      cwd: UI3,
      env,
      stdio: "pipe",
    });
  } catch (err) {
    console.error(`check-perf-parity: the ${mode} capture run failed`);
    console.error(String(err.stdout ?? ""));
    console.error(String(err.stderr ?? ""));
    process.exit(1);
  }
  return JSON.parse(readFileSync(file, "utf8"));
}

const ABSENT = Symbol("absent");

function kindOf(v) {
  if (Array.isArray(v)) return "array";
  if (v === null) return "null";
  return typeof v;
}

/** Every leaf where the two encoded captures disagree, as dotted paths. */
function diff(a, b, path = "", out = []) {
  if (a === ABSENT || b === ABSENT) {
    out.push({ path, left: a, right: b });
    return out;
  }
  const ka = kindOf(a);
  if (ka !== kindOf(b)) {
    out.push({ path, left: a, right: b });
    return out;
  }
  if (ka === "array") {
    const n = Math.max(a.length, b.length);
    for (let i = 0; i < n; i++) {
      diff(i < a.length ? a[i] : ABSENT, i < b.length ? b[i] : ABSENT, `${path}[${i}]`, out);
    }
    return out;
  }
  if (ka === "object") {
    for (const k of [...new Set([...Object.keys(a), ...Object.keys(b)])].sort()) {
      const p = path ? `${path}.${k}` : k;
      diff(
        Object.hasOwn(a, k) ? a[k] : ABSENT,
        Object.hasOwn(b, k) ? b[k] : ABSENT,
        p,
        out,
      );
    }
    return out;
  }
  if (!Object.is(a, b)) out.push({ path, left: a, right: b });
  return out;
}

function show(v) {
  if (v === ABSENT) return "(key absent)";
  const s = JSON.stringify(v);
  return s === undefined ? String(v) : s.length > 160 ? `${s.slice(0, 157)}...` : s;
}

/** Whether every element of `want` appears in `got`, in order and unchanged. */
function isSubsequence(want, got) {
  const missing = [];
  let j = 0;
  for (const item of want) {
    const target = JSON.stringify(item);
    let found = false;
    while (j < got.length) {
      if (JSON.stringify(got[j++]) === target) {
        found = true;
        break;
      }
    }
    if (!found) missing.push(item);
  }
  return missing;
}

console.log("check-perf-parity: capturing default mode (validation on)...");
const base = capture("default");
console.log("check-perf-parity: capturing DCL_PERF=1 (validation stripped)...");
const perf = capture("perf");

// Before comparing anything: prove the two captures are actually the two modes.
// A comparison of a mode against itself is the one failure this gate could have
// that looks exactly like success.
const setup = [];
if (base.mode !== "default" || base.schemasStubbed || !base.validationEnabled) {
  setup.push(
    `the default capture did not resolve the checking build ` +
      `(mode=${base.mode}, schemasStubbed=${base.schemasStubbed}, validationEnabled=${base.validationEnabled})`,
  );
}
if (perf.mode !== "perf" || !perf.schemasStubbed || perf.validationEnabled) {
  setup.push(
    `the DCL_PERF=1 capture did not resolve the stubbed build ` +
      `(mode=${perf.mode}, schemasStubbed=${perf.schemasStubbed}, validationEnabled=${perf.validationEnabled}) ` +
      `\u{2014} check that DCL_PERF reached the vitest config`,
  );
}
if (base.caseCount === 0) setup.push("no cases ran; the harness is measuring nothing");
if (base.caseCount !== perf.caseCount) {
  setup.push(`case counts differ: ${base.caseCount} vs ${perf.caseCount}`);
}
if (setup.length > 0) {
  console.error("check-perf-parity: HARNESS BROKEN");
  for (const s of setup) console.error(`  ${s}`);
  process.exit(2);
}

// Restated here rather than imported: this script is plain node and cases.ts is
// TypeScript behind the vite pipeline. The capture carries each case's tags, so
// the mapping stays in one place even though the prose lives in two.
const LEGEND = {
  "federated-cdn":
    "the cdn.decentraland.org -> federated CDN rewrite stops running, so thumbnails load " +
    "from the PROD CDN. play.catalyst.example.com must reach catalyst.example.com only, so this is a " +
    "federation break, not a cosmetic one (schemas/communities.ts:16-22)",
  normalization:
    "transforms stop running: `nullish().transform((v) => v ?? null)` leaves the field " +
    "undefined where the type says null, and the emote-category reshape stops filing " +
    "unknown categories under the catch-all (schemas/backpack.ts:87)",
  "bad-row":
    "a malformed row the default build DROPS reaches the view mapper, which throws on a " +
    "field the row does not have and takes the whole read down with it",
  "row-guard":
    "a reader's structural guard (src/data/catalyst/rows.ts) is what keeps an unusable row " +
    "out of a mapper once the schema is gone. If a guarded case diverges, either the guard " +
    "did not run in perf mode or it is not stated where both modes can reach it",
  "key-strip":
    "a reader that hands its schema more keys than the shape declares relies on zod " +
    "stripping them; the accepting stub returns them all",
};

const failures = [];
const byProbe = new Map();

for (let i = 0; i < base.cases.length; i++) {
  const a = base.cases[i];
  const b = perf.cases.find((c) => c.id === a.id);
  if (!b) {
    failures.push({ case: a, reasons: [`missing from the perf capture`] });
    continue;
  }
  const reasons = [];

  // `guarded` is compared exactly like `parity`: in both, every row either
  // survives in BOTH modes or is dropped in both, so any difference at all is a
  // step the perf build stopped taking.
  if (a.group === "parity" || a.group === "guarded") {
    if (a.outcome !== b.outcome) {
      reasons.push(
        `default ${a.outcome === "threw" ? `threw (${a.error})` : "returned a value"}, ` +
          `perf ${b.outcome === "threw" ? `threw (${b.error})` : "returned a value"}`,
      );
    } else if (a.outcome === "threw") {
      if (a.error !== b.error) reasons.push(`both threw, differently: "${a.error}" vs "${b.error}"`);
    } else {
      for (const d of diff(a.value, b.value)) {
        reasons.push(`${d.path || "(root)"}\n        default: ${show(d.left)}\n        perf:    ${show(d.right)}`);
      }
    }
  } else {
    if (a.outcome === "threw") reasons.push(`the default build threw: ${a.error}`);
    if (b.outcome === "threw") {
      reasons.push(
        `perf threw where default returned: ${b.error}\n` +
          `        the malformed row reached a view mapper; the valid rows are lost with it`,
      );
    }
    if (a.outcome === "returned" && b.outcome === "returned") {
      if (Array.isArray(a.value) && Array.isArray(b.value)) {
        for (const m of isSubsequence(a.value, b.value)) {
          reasons.push(`a valid row the default build returned is missing or altered in perf:\n        ${show(m)}`);
        }
      } else {
        for (const d of diff(a.value, b.value)) {
          reasons.push(`${d.path || "(root)"}: ${show(d.left)} vs ${show(d.right)}`);
        }
      }
    }
  }

  if (reasons.length > 0) {
    failures.push({ case: a, reasons });
    for (const p of a.probes) byProbe.set(p, [...(byProbe.get(p) ?? []), a.id]);
  }
}

// A differential is blind in one direction, and it is the direction that
// matters: delete a normalization outright and BOTH modes lose it, so they
// still agree and this gate stays green. Confirmed by probe -- removing the
// rewrite in normalizeCommunityThumbnail left this reporting 27/27 identical
// while pointing community thumbnails back at the prod CDN.
//
// Anchors are the other half: statements about the RIGHT answer in default
// mode, not merely a matching one. Checked against the DEFAULT capture, because
// perf mode is allowed to check less -- it is not allowed to normalize less,
// and that is what the anchors pin.
// Evaluated inside the capture rather than here: an anchor's `select` is a
// function, which cannot cross the JSON boundary between the vitest process and
// this runner. The capture reports the verdict; this reports the verdict.
const anchorFailures = base.anchorFailures ?? null;
if (anchorFailures === null) {
  console.error(
    "check-perf-parity: the default capture carries no anchor verdict \u{2014} the capture is older " +
      "than this runner, or anchor evaluation was removed. Refusing to report success on a " +
      "differential alone.",
  );
  process.exit(1);
}

const passed = base.cases.length - failures.length;
console.log(
  `check-perf-parity: ${base.cases.length} case(s), ${passed} identical across modes, ${failures.length} divergent`,
);
console.log(
  `check-perf-parity: ${base.anchorCount ?? 0} anchor(s) on default-mode output, ${anchorFailures.length} wrong`,
);

if (anchorFailures.length > 0) {
  console.error("");
  console.error("check-perf-parity: FAILED \u{2014} default mode produces the WRONG value");
  console.error("A normalization is missing from BOTH modes, so the differential above cannot see it.");
  for (const f of anchorFailures) {
    console.error("");
    console.error(`  ${f.id}`);
    console.error(`      ${f.detail}`);
    console.error(`      why it matters: ${f.why}`);
  }
  process.exit(1);
}

if (failures.length === 0) {
  if (!keepAt) rmSync(outDir, { recursive: true, force: true });
  console.log("check-perf-parity: OK \u{2014} perf mode changes what is checked and nothing else");
  process.exit(0);
}

console.error("");
console.error("check-perf-parity: FAILED \u{2014} performance mode changes reader OUTPUT, not just checking");
console.error("");
for (const f of failures) {
  console.error(`  ${f.case.id}  [${f.case.group}${f.case.probes.length ? `: ${f.case.probes.join(", ")}` : ""}]`);
  console.error(`      ${f.case.note}`);
  for (const r of f.reasons) console.error(`      ${r}`);
  console.error("");
}

console.error("  divergences observed:");
for (const [probe, text] of Object.entries(LEGEND)) {
  const hits = byProbe.get(probe);
  if (!hits) continue;
  console.error(`    ${probe} \u{2014} ${text}`);
  console.error(`      proven by: ${[...new Set(hits)].join(", ")}`);
}
const clean = Object.keys(LEGEND).filter((p) => !byProbe.has(p));
if (clean.length > 0) console.error(`  not observed in this run: ${clean.join(", ")}`);
console.error("");
console.error("  fix, for a parity case: move the transform out of src/data/catalyst/schemas/* into the");
console.error("  reader, so a schema module carries SHAPE ONLY and replacing it with an accepting stub");
console.error("  changes what is CHECKED and nothing else.");
console.error("  fix, for a guarded case: state the mapper's precondition as a guard in the reader");
console.error("  (src/data/catalyst/rows.ts) instead of leaning on the schema to reject the row.");
console.error("  fix, for a robustness case: the malformed row reached a mapper that could not survive");
console.error("  it \u{2014} widen the guard to name the field the mapper dereferences. Captures kept at:");
console.error(`    ${join(outDir, "default.json")}`);
console.error(`    ${join(outDir, "perf.json")}`);
process.exit(1);
