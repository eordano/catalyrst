// Emit the perf-mode stand-ins for the pure schema modules.
//
// In performance mode nothing is validated, so the schemas only still exist to
// satisfy the import. A stub exporting the same names keeps every call site
// compiling while removing the last edge to zod, which is what lets the bundler
// drop it.
//
// Each export is an always-accepting shim rather than `null`, because not every
// schema is consumed through `check`: the catalyst readers call `.safeParse` and
// `.parse` on theirs directly, and a null there would make the perf build throw
// where the default build merely dropped a bad row. Accepting everything is what
// the mode already means -- see src/validate/unchecked.ts, whose `check` returns
// its argument unvalidated for the same reason.
//
// Generated rather than written because the failure mode is invisible: a schema
// added to the real module and missing from a hand-written stub breaks only the
// perf build, which nobody runs by default. Deriving the names from the real
// module means that cannot happen -- and `--check` makes the staleness a gate
// failure rather than something found at release time.
//
//   node scripts/gen-schema-stubs.mts [--check]

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const UI3 = new URL("..", import.meta.url);

/** Real module -> the stub that replaces it in perf mode. */
const TARGETS = [
  ["src/generated/bridge-schemas.ts", "src/generated/bridge-schemas.stub.ts"],
  ["src/generated/editor-bus-schemas.ts", "src/generated/editor-bus-schemas.stub.ts"],
  ["src/data/persisted-schemas.ts", "src/data/persisted-schemas.stub.ts"],
  ["src/data/auth/thirdwebSchema.ts", "src/data/auth/thirdwebSchema.stub.ts"],
  ["src/data/catalyst/schemas/backpack.ts", "src/data/catalyst/schemas/backpack.stub.ts"],
  ["src/data/catalyst/schemas/communities.ts", "src/data/catalyst/schemas/communities.stub.ts"],
  ["src/data/catalyst/schemas/events.ts", "src/data/catalyst/schemas/events.stub.ts"],
  ["src/data/catalyst/schemas/notifications.ts", "src/data/catalyst/schemas/notifications.stub.ts"],
  ["src/data/catalyst/schemas/places.ts", "src/data/catalyst/schemas/places.stub.ts"],
  ["src/data/catalyst/schemas/profile.ts", "src/data/catalyst/schemas/profile.stub.ts"],
];

// Only top-level `export const <Name>` -- the generated modules also export
// `_Assert*` TYPES, which are erased and must not appear in a stub.
const EXPORT_RE = /^export const ([A-Za-z0-9_$]+)/gm;

function stubFor(sourceRel, source) {
  const names = [...source.matchAll(EXPORT_RE)].map((m) => m[1]);
  if (names.length === 0) {
    console.error(`gen-schema-stubs: no exports found in ${sourceRel} -- refusing to write an empty stub`);
    process.exit(1);
  }
  return [
    `// GENERATED from ${sourceRel} by catalyrst/ui3/scripts/gen-schema-stubs.mts. Do not edit.`,
    "//",
    "// Performance-mode stand-in: the perf build aliases the real module here so",
    "// zod leaves the bundle. Every export is the same always-accepting shim, so a",
    "// call site that parses its schema directly keeps working and one that hands it",
    "// to `check` never looks at it -- see src/validate/unchecked.ts.",
    "//",
    "// Accepting everything is the trade the mode makes, and it is a real one. The",
    "// transforms go with the schemas, so a nullish field stays undefined instead of",
    "// normalizing to null; and a reader that used validation to DROP a bad row now",
    "// hands that row to its view mapper, which can throw on a field the row does not",
    "// have. Performance mode trusts the wire -- turn it on only where that holds.",
    "",
    "const accept = {",
    "  parse: (value: unknown) => value,",
    "  safeParse: (value: unknown) => ({ success: true as const, data: value }),",
    "} as never;",
    "",
    ...names.map((n) => `export const ${n} = accept;`),
    "",
  ].join("\n");
}

const check = process.argv.includes("--check");
let drifted = 0;

for (const [sourceRel, stubRel] of TARGETS) {
  const sourcePath = fileURLToPath(new URL(sourceRel, UI3));
  const stubPath = fileURLToPath(new URL(stubRel, UI3));
  const wanted = stubFor(sourceRel, readFileSync(sourcePath, "utf8"));

  if (!check) {
    writeFileSync(stubPath, wanted);
    console.log(`gen-schema-stubs: wrote ${stubRel}`);
    continue;
  }
  let current = null;
  try {
    current = readFileSync(stubPath, "utf8");
  } catch {}
  if (current !== wanted) {
    console.error(`gen-schema-stubs: ${stubRel} is stale or missing`);
    drifted++;
  }
}

if (check) {
  if (drifted > 0) {
    console.error("fix: cd catalyrst/ui3 && node scripts/gen-schema-stubs.mts   # then commit");
    process.exit(1);
  }
  console.log(`gen-schema-stubs: ${TARGETS.length} stub(s) up to date`);
}
