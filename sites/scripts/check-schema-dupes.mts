/**
 * Flags hand-written zod object schemas that shadow a generated one.
 *
 * The erosion class this exists to stop, found 40+ times before the
 * generated-schemas/ migration: a file needs to parse a catalyst payload,
 * the generated schema is two imports away, and someone re-declares it --
 *
 *     const PlaceSchema = z.object({ id: z.string(), title: z.string(), ... });
 *
 * -- property for property. The copy then drifts silently: the Rust type gains
 * a field, ts-rs and gen-zod-schemas.mts propagate it, and the hand copy keeps
 * validating yesterday's shape while claiming today's name.
 *
 * Detection is by property-name overlap, not by name or by type: every
 * `z.object({...})` literal in packages/data/src/lib/catalyst/generated-schemas/
 * (top-level and nested, attributed to its export) is indexed by its
 * property-name set, and every `z.object({...})` declared anywhere else is
 * compared against that index. A Jaccard overlap >= 0.9 -- identical or
 * near-identical property sets -- is a duplicate. Small shapes (< 3 named
 * properties) are ignored on both sides: {x, y} is a coordinate, not a copy.
 *
 * Machine-written files (a "do not edit" marker in the header) are skipped:
 * a generator emitting a mirror of another generator's output is provenance,
 * not erosion. Objects containing spreads are skipped too -- their full
 * property set is unknowable without type resolution, and guessing invites
 * false flags.
 *
 *   node scripts/check-schema-dupes.mts              # report
 *   node scripts/check-schema-dupes.mts --check      # exit 1 on any new duplicate
 *   node scripts/check-schema-dupes.mts --baseline   # rewrite the exception list (keeps reasons)
 *
 * Fix for a flagged site: import the generated schema (or the relevant piece
 * of it) from @data/lib/catalyst/generated-schemas instead of re-declaring it.
 *
 * schema-dupes-baseline.json is NOT a debt list any more. The pre-existing
 * debt was burned to zero (every hand copy now imports or derives from the
 * generated schema); what remains are deliberately-durable exceptions, and
 * every entry MUST carry a one-line `reason` saying why the duplicate cannot
 * or must not be fixed by import -- `--check` fails on a reasonless entry.
 * The two standing exception classes:
 *   - ui3/src/data: ui3 deliberately carries its own schema tree and has no
 *     import path into sites' generated-schemas (the dependency points the
 *     other way -- sites imports ui3's ts-rs types).
 *   - shape collisions: a hand schema for a service that has no ts-rs export
 *     of that shape, flagged only because it happens to share its property
 *     names with an unrelated generated schema.
 */
import { readdirSync, readFileSync, existsSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import type * as TSNamespace from "typescript6";

const require = createRequire(import.meta.url);
const ts: typeof TSNamespace = require("typescript6");

const SITES = fileURLToPath(new URL("..", import.meta.url));
const GENERATED_DIR = join(
  SITES, "packages", "data", "src", "lib", "catalyst", "generated-schemas",
);
const SCAN_ROOTS = [
  join(SITES, "app"),
  join(SITES, "packages"),
  join(SITES, "scripts"),
  join(SITES, "..", "ui3", "src"),
];
const BASELINE_PATH = join(SITES, "scripts", "schema-dupes-baseline.json");
const SKIP_DIRS = new Set(["node_modules", "__snapshots__", "generated-schemas"]);
const OBJECT_CALLS = new Set(["object", "strictObject", "looseObject"]);
const MIN_PROPS = 3;
const THRESHOLD = 0.9;

interface GeneratedShape {
  exportName: string;
  file: string;
  props: Set<string>;
}

interface Duplicate {
  file: string;
  line: number;
  props: number;
  match: GeneratedShape;
  overlap: number;
}

function fail(msg: string): never {
  console.error(`check-schema-dupes: ${msg}`);
  process.exit(1);
}

function tsFiles(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
    a.name.localeCompare(b.name),
  )) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) continue;
      tsFiles(full, out);
    } else if (/\.(ts|tsx|mts)$/.test(entry.name) && !entry.name.endsWith(".d.ts")) {
      out.push(full);
    }
  }
  return out;
}

function isMachineWritten(text: string): boolean {
  return /do not edit/i.test(text.slice(0, 400));
}

/** The property-name set of a z.object argument, or null when unknowable. */
function propNames(
  call: TSNamespace.CallExpression,
): Set<string> | null {
  const arg = call.arguments[0];
  if (!arg || !ts.isObjectLiteralExpression(arg)) return null;
  const names = new Set<string>();
  for (const p of arg.properties) {
    if (ts.isSpreadAssignment(p)) return null;
    const name = p.name;
    if (!name) return null;
    if (ts.isIdentifier(name) || ts.isStringLiteral(name)) names.add(name.text);
    else return null;
  }
  return names;
}

function isZodObjectCall(node: TSNamespace.Node): node is TSNamespace.CallExpression {
  return (
    ts.isCallExpression(node) &&
    ts.isPropertyAccessExpression(node.expression) &&
    OBJECT_CALLS.has(node.expression.name.text) &&
    ts.isIdentifier(node.expression.expression) &&
    node.expression.expression.text === "z"
  );
}

/** Every z.object property set in a generated module, tagged with its export. */
function generatedShapes(file: string): GeneratedShape[] {
  const sf = ts.createSourceFile(
    file, readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true,
  );
  const rel = relative(SITES, file);
  const out: GeneratedShape[] = [];
  const collect = (node: TSNamespace.Node, exportName: string): void => {
    if (isZodObjectCall(node)) {
      const props = propNames(node);
      if (props && props.size >= MIN_PROPS) out.push({ exportName, file: rel, props });
    }
    ts.forEachChild(node, (c) => collect(c, exportName));
  };
  for (const stmt of sf.statements) {
    if (!ts.isVariableStatement(stmt)) continue;
    for (const decl of stmt.declarationList.declarations) {
      if (!ts.isIdentifier(decl.name) || !decl.initializer) continue;
      collect(decl.initializer, decl.name.text);
    }
  }
  return out;
}

function jaccard(a: Set<string>, b: Set<string>): number {
  let shared = 0;
  for (const x of a) if (b.has(x)) shared++;
  return shared / (a.size + b.size - shared);
}

function scan(file: string, index: GeneratedShape[]): Duplicate[] {
  const text = readFileSync(file, "utf8");
  if (!text.includes("z.")) return [];
  if (isMachineWritten(text)) return [];
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const rel = relative(SITES, file);
  const out: Duplicate[] = [];
  const visit = (node: TSNamespace.Node): void => {
    if (isZodObjectCall(node)) {
      const props = propNames(node);
      if (props && props.size >= MIN_PROPS) {
        let best: GeneratedShape | null = null;
        let bestOverlap = 0;
        for (const shape of index) {
          const o = jaccard(props, shape.props);
          if (o > bestOverlap) {
            bestOverlap = o;
            best = shape;
          }
        }
        if (best && bestOverlap >= THRESHOLD) {
          const { line } = sf.getLineAndCharacterOfPosition(node.getStart(sf));
          out.push({
            file: rel,
            line: line + 1,
            props: props.size,
            match: best,
            overlap: bestOverlap,
          });
        }
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
  return out;
}

interface BaselineRow {
  file: string;
  duplicates: string;
  reason?: string;
}

/**
 * Baseline entries are keyed by file + duplicated export, not by line: these
 * trees are edited concurrently, and a line-keyed entry breaks every time an
 * unrelated edit shifts the file. The coarser key means a second copy of the
 * SAME schema in the same file rides an existing entry -- acceptable, since the
 * first copy is already a recorded exception -- while a copy of any other
 * schema, or in any other file, still fails.
 */
function baselineRows(): BaselineRow[] {
  if (!existsSync(BASELINE_PATH)) return [];
  const parsed: unknown = JSON.parse(readFileSync(BASELINE_PATH, "utf8"));
  return Array.isArray(parsed) ? (parsed as BaselineRow[]) : [];
}

function main(): void {
  const check = process.argv.includes("--check");
  const writeBaseline = process.argv.includes("--baseline");

  if (!existsSync(GENERATED_DIR)) fail(`missing ${GENERATED_DIR}`);
  const index = readdirSync(GENERATED_DIR)
    .sort()
    .filter((e) => e.endsWith(".ts"))
    .flatMap((e) => generatedShapes(join(GENERATED_DIR, e)));
  if (index.length === 0) {
    fail(
      "no z.object shapes found in generated-schemas/ \u{2014} a scan with an empty index cannot flag anything and must not report success",
    );
  }

  const all = SCAN_ROOTS.filter((r) => existsSync(r))
    .flatMap((r) => tsFiles(r))
    .flatMap((f) => scan(f, index));

  if (writeBaseline) {
    const prior = new Map(
      baselineRows().map((r) => [`${r.file}#${r.duplicates}`, r.reason]),
    );
    const rows = [...new Map(
      all.map((d) => {
        const key = `${d.file}#${d.match.exportName}`;
        const reason = prior.get(key);
        return [key, { file: d.file, duplicates: d.match.exportName, ...(reason ? { reason } : {}) }];
      }),
    ).values()];
    writeFileSync(BASELINE_PATH, `${JSON.stringify(rows, null, 2)}\n`);
    console.log(`check-schema-dupes: baselined ${rows.length} duplicate(s) to`);
    console.log(`  ${relative(SITES, BASELINE_PATH)}`);
    console.log(
      `Every entry needs a one-line "reason" saying why it cannot be fixed by\n` +
        `importing the generated schema \u{2014} --check fails on reasonless entries.`,
    );
    return;
  }

  const rows = baselineRows();
  const unreasoned = rows.filter((r) => !r.reason?.trim());
  if (unreasoned.length > 0) {
    console.error(
      `check-schema-dupes: ${unreasoned.length} baseline entr${unreasoned.length === 1 ? "y" : "ies"} without a reason\n`,
    );
    for (const r of unreasoned) console.error(`  ${r.file}#${r.duplicates}`);
    console.error(
      `\nThe baseline holds deliberate exceptions only. Either fix the duplicate` +
        `\nby importing the generated schema, or add a one-line "reason" field` +
        `\nsaying why this copy must stay hand-written.`,
    );
    if (check) process.exit(1);
  }
  const baseline = new Set(rows.map((r) => `${r.file}#${r.duplicates}`));
  const dupes = all.filter((d) => !baseline.has(`${d.file}#${d.match.exportName}`));

  if (dupes.length === 0) {
    console.log(
      `check-schema-dupes: OK (${index.length} generated shapes indexed, ` +
        `${baseline.size} durable exception(s), each with a reason)`,
    );
    return;
  }

  console.error(
    `check-schema-dupes: ${dupes.length} hand-written z.object declaration(s) duplicate a generated schema\n`,
  );
  for (const d of dupes) {
    console.error(
      `  ${d.file}:${d.line}  ${d.props} props, ${Math.round(d.overlap * 100)}% of ` +
        `${d.match.exportName} (${d.match.file})`,
    );
  }
  console.error(
    `\nA hand-written copy of a generated schema drifts silently when the Rust` +
      `\ntype changes. Import the generated schema from` +
      `\n  packages/data/src/lib/catalyst/generated-schemas/` +
      `\ninstead of re-declaring its shape.`,
  );
  process.exit(check ? 1 : 0);
}

main();
