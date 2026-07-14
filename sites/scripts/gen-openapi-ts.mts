import fs from "node:fs";
import path from "node:path";
import openapiTS from "openapi-typescript";

const SPEC_SET_MISMATCH = 3;

const [, , dir, ...expected] = process.argv;
if (!dir || !fs.existsSync(dir) || expected.length === 0) {
  console.error(
    "usage: gen-openapi-ts.mts <dir-with-*.openapi.json> <spec-name>...",
  );
  console.error(
    "spec names come from generated-artefacts.mts --list catalyrst-openapi",
  );
  process.exit(2);
}

const specs = fs
  .readdirSync(dir)
  .filter((f) => f.endsWith(".openapi.json"))
  .sort();

const found = specs.map((f) => f.replace(/\.openapi\.json$/, ""));
const want = [...new Set(expected)].sort();
const missing = want.filter((n) => !found.includes(n));
const unlisted = found.filter((n) => !want.includes(n));
if (missing.length || unlisted.length) {
  console.error(
    `SPEC SET DRIFT: ${dir} does not hold the declared OpenAPI specs.`,
  );
  if (missing.length) {
    console.error(`  declared but not generated: ${missing.join(" ")}`);
  }
  if (unlisted.length) {
    console.error(`  generated but not declared: ${unlisted.join(" ")}`);
  }
  console.error(
    "Fix [package.metadata.generated] openapi in the emitting crate's Cargo.toml and",
  );
  console.error("commit the matching catalyrst/ui3/src/generated/catalyst/openapi/ files.");
  process.exit(SPEC_SET_MISMATCH);
}

for (const f of specs) {
  const spec = JSON.parse(fs.readFileSync(path.join(dir, f), "utf8"));
  const ts = await openapiTS(spec, { alphabetize: true });
  const out = path.join(dir, f.replace(/\.openapi\.json$/, ".ts"));
  fs.writeFileSync(out, ts);
  console.log(`openapi -> ${out}`);
}
