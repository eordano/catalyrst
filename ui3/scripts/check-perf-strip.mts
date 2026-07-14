// Prove that a performance-mode build actually removed the validation layer.
//
// The whole point of aliasing modules rather than branching at runtime is that
// zod and the schema definitions leave the bundle. Nothing about that is
// self-evident from the source: an alias that silently fails to match, an
// import added later through a path the alias does not cover, or a bundler
// change all produce a build that pays the bytes while looking correct. That is
// the worst outcome the mode has, so it is asserted rather than assumed.
//
// Both builds run, because two things must hold and they fail in opposite
// directions: the perf bundle must NOT contain zod, and the default bundle
// MUST. Without the second assertion a broken build that emitted nothing, or an
// alias applied unconditionally, would pass the first one trivially.
//
// Full removal is reached, so this asserts it rather than ratcheting toward it:
// a recorded list of allowed failures is only worth keeping while it is
// shrinking; kept past zero it is just a way to record the next regression as
// acceptable.
//
//   node scripts/check-perf-strip.mts

import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const UI3 = fileURLToPath(new URL("..", import.meta.url));
// NOT dist-overlay: scripts/publish-overlay.mts ships from that directory, and
// this script's last build is the DCL_PERF=1 one. Sharing the path means running
// the gate and then publishing uploads a bundle with validation stripped out --
// silently, since a perf build differs only by what it does NOT do.
const OUT = join(UI3, "dist-perf-check");

// A zod bundle keeps these strings: the first is zod 4's internal registry
// symbol, the second its parse error class name. Matching two independent
// markers rather than the word "zod", which appears in comments and paths.
const ZOD_MARKERS = ["$ZodError", "_zod"];

function build(perf) {
  rmSync(OUT, { recursive: true, force: true });
  // --outDir overrides the config's dist-overlay. The config itself cannot be
  // changed instead: dist-overlay is what publish-overlay.mts ships.
  execFileSync("npx", ["vite", "build", "--config", "vite.overlay.config.js", "--outDir", OUT], {
    cwd: UI3,
    env: { ...process.env, DCL_PERF: perf ? "1" : "" },
    stdio: "pipe",
  });
  // The overlay emits overlay.js at the root plus chunks/, not an assets/ dir
  // of scripts -- assets/ is images. Walk everything and take the .js.
  let bytes = 0;
  let text = "";
  let files = 0;
  for (const entry of readdirSync(OUT, { recursive: true })) {
    if (typeof entry !== "string" || !entry.endsWith(".js")) continue;
    const buf = readFileSync(join(OUT, entry));
    bytes += buf.length;
    text += buf.toString("utf8");
    files++;
  }
  if (files === 0) throw new Error(`no .js emitted into ${OUT} \u{2014} the build produced nothing to check`);
  return { bytes, hits: ZOD_MARKERS.filter((m) => text.includes(m)) };
}

// Modules the perf build already replaces via vite.validate.js. Their source
// imports zod and their alias does not, so counting them here would report a
// blocker that is already solved.
const ALIASED = new Set([
  "generated/bridge-schemas.ts",
  "generated/editor-bus-schemas.ts",
  "data/persisted-schemas.ts",
  "data/auth/thirdwebSchema.ts",
  "data/catalyst/schemas/backpack.ts",
  "data/catalyst/schemas/communities.ts",
  "data/catalyst/schemas/events.ts",
  "data/catalyst/schemas/notifications.ts",
  "data/catalyst/schemas/places.ts",
  "data/catalyst/schemas/profile.ts",
]);

// Every module that still pulls zod into a perf build. `import type` does not
// count -- it is erased -- so this looks for value imports only.
//
// Tests are skipped for the same reason stubs are: neither is in the answer. A
// test file has no entry point into either build, so a value import of zod there
// costs a perf bundle nothing -- and reporting one is worse than useless, since
// the message it prints tells the reader to go extract schemas out of a module
// that ships nowhere. `test/validate-seam.test.ts` imports zod to build the
// schemas it checks `check()` against, and cannot not.
function zodImporters() {
  const out = [];
  for (const entry of readdirSync(join(UI3, "src"), { recursive: true })) {
    if (typeof entry !== "string" || !/\.tsx?$/.test(entry)) continue;
    if (entry.endsWith(".stub.ts")) continue;
    if (/\.(test|parity)\.tsx?$/.test(entry)) continue;
    if (ALIASED.has(entry.replaceAll("\\", "/"))) continue;
    const text = readFileSync(join(UI3, "src", entry), "utf8");
    if (/^import\s+(?!type\s)[^;]*from\s+["\']zod["\']/m.test(text)) {
      out.push(entry.replaceAll("\\", "/"));
    }
  }
  return out.sort();
}

const importers = zodImporters();

console.log("check-perf-strip: building default (validation on)...");
const normal = build(false);
console.log("check-perf-strip: building DCL_PERF=1 (validation stripped)...");
const perf = build(true);

const saved = normal.bytes - perf.bytes;
console.log(
  `check-perf-strip: default ${(normal.bytes / 1024).toFixed(1)} KB, ` +
    `perf ${(perf.bytes / 1024).toFixed(1)} KB, saved ${(saved / 1024).toFixed(1)} KB`,
);

const problems = [];
if (normal.hits.length === 0) {
  problems.push(
    `default build contains no zod markers (${ZOD_MARKERS.join(", ")}) \u{2014} either validation is not wired in, or these markers no longer identify zod and this check proves nothing`,
  );
}

// The assertion the whole file exists for. The source scan runs alongside it
// because the two answer different questions: the markers say zod is in the
// bundle, the scan says which module put it there. A marker hit with an empty
// importer list means the edge came through node_modules or through a
// specifier the alias does not match, which is the case worth naming.
if (perf.hits.length > 0) {
  problems.push(
    `perf build still contains zod (${perf.hits.join(", ")})\n` +
      (importers.length > 0
        ? `  these module(s) import it at runtime and are not aliased:\n` +
          importers.map((f) => `    src/${f}`).join("\n") +
          "\n  extract the schemas into a pure module under a schemas/ directory, then register it in\n" +
          "  vite.validate.js, scripts/gen-schema-stubs.mts and the ALIASED set here"
        : "  no source module imports zod, so the edge is a dependency's or an alias that stopped matching \u{2014}\n" +
          "  check the specifiers in vite.validate.js against how the schema modules are actually imported"),
  );
} else if (importers.length > 0) {
  // Unreachable in a correct build and worth saying so: a module importing zod
  // whose bytes are absent means the bundler dropped code it should not have.
  problems.push(
    `perf build has no zod markers, yet these modules import it: ${importers.join(", ")}`,
  );
} else {
  console.log("check-perf-strip: zod fully removed from the perf build");
}

if (perf.bytes >= normal.bytes) {
  problems.push(`perf build is not smaller (${perf.bytes} >= ${normal.bytes} bytes)`);
}

if (problems.length > 0) {
  console.error("check-perf-strip: FAILED");
  for (const p of problems) console.error("  " + p);
  process.exit(1);
}
console.log("check-perf-strip: OK");
