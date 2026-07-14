import ts from "typescript6";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SITES = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const APP = path.join(SITES, "packages");
const CORE = path.join(APP, "core", "src");
const EVENTS_DIR = path.join(CORE, "lib", "telemetry", "events");
const EVENTS_ENTRY = path.join(CORE, "lib", "telemetry", "events.ts");
const CONTRACT_PATH = path.join(CORE, "lib", "telemetry", "telemetry-contract.json");

const mode = process.argv.includes("--check")
  ? "check"
  : process.argv.includes("--draft")
    ? "draft"
    : process.argv.includes("--write")
      ? "write"
      : process.argv.includes("--contract")
        ? "contract"
        : "report";

type Surface = [slug: string, typeName: string, match: RegExp];

const SURFACES: Surface[] = [
  ["client", "ClientEvents", /^cl_/],
  ["creator-hub", "CreatorHubEvents", /^(ch|bd|creator|create)_/],
  ["governance", "GovernanceEvents", /^gv_/],
  ["landings", "LandingsEvents", /^(lp|landings?|cast|report|jump)_/],
  ["marketplace", "MarketplaceEvents", /^(mk|cart|pack)_/],
  ["operator-admin", "OperatorAdminEvents", /^(operator|admin)_/],
  ["misc", "MiscEvents", /./],
];

type PropInfo = { name: string; type: string; optional: boolean };
type Shape = { loose: boolean; props: PropInfo[] };
type PropAcc = { types: Set<string>; present: number; optional: boolean };
type EventEntry = { props: Map<string, PropAcc>; sites: string[]; shapeCount: number };
type DynamicEntry = { site: string; callee: string; argType?: string };
type HarvestResult = { events: Map<string, EventEntry>; dynamic: DynamicEntry[] };

function loadProgram(): ts.Program {
  const configPath = path.join(SITES, "tsconfig.json");
  const host: ts.ParseConfigFileHost = {
    ...ts.sys,
    onUnRecoverableConfigFileDiagnostic: (d: ts.Diagnostic) => {
      throw new Error(ts.flattenDiagnosticMessageText(d.messageText, "\n"));
    },
  };
  const parsed = ts.getParsedCommandLineOfConfigFile(configPath, {}, host);
  if (!parsed) throw new Error(`failed to parse ${configPath}`);
  return ts.createProgram({
    rootNames: parsed.fileNames,
    options: parsed.options,
  });
}

function isTrackContextParam(checker: ts.TypeChecker, sig: ts.Signature, index: number): boolean {
  const p = sig.parameters[index];
  if (!p) return false;
  const decl = p.valueDeclaration ?? p.declarations?.[0];
  if (!decl) return false;
  const t = checker.getTypeOfSymbolAtLocation(p, decl);
  return checker.typeToString(t) === "TrackContext";
}

function calleeName(node: ts.CallExpression): string | null {
  const e = node.expression;
  if (ts.isIdentifier(e)) return e.text;
  if (ts.isPropertyAccessExpression(e)) return e.name.text;
  return null;
}

function isForwardedParam(checker: ts.TypeChecker, expr: ts.Expression): boolean {
  if (!ts.isIdentifier(expr)) return false;
  const sym = checker.getSymbolAtLocation(expr);
  const decl = sym?.valueDeclaration;
  return !!decl && ts.isParameter(decl);
}

function isCastExpr(expr: ts.Expression): boolean {
  let e: ts.Expression = expr;
  while (ts.isParenthesizedExpression(e)) e = e.expression;
  return (
    ts.isAsExpression(e) ||
    ts.isTypeAssertionExpression(e) ||
    ts.isSatisfiesExpression(e)
  );
}

function stringLiterals(checker: ts.TypeChecker, expr: ts.Expression): string[] | null {
  const t = checker.getTypeAtLocation(expr);
  if (t.isStringLiteral()) return [t.value];
  if (t.isUnion()) {
    const out: string[] = [];
    for (const m of t.types) {
      if (!m.isStringLiteral()) return null;
      out.push(m.value);
    }
    return out;
  }
  return null;
}

function typeText(checker: ts.TypeChecker, t: ts.Type): string {
  if (t.isUnion()) {
    const parts = t.types.map((m) => typeText(checker, m));
    return [...new Set(parts)].join(" | ");
  }
  if (t.isStringLiteral()) return JSON.stringify(t.value);
  if (t.isNumberLiteral()) return String(t.value);
  if (t.flags & ts.TypeFlags.BooleanLiteral) return checker.typeToString(t);
  return checker.typeToString(t, undefined, ts.TypeFormatFlags.NoTruncation);
}

function propsShape(checker: ts.TypeChecker, expr: ts.Expression): Shape {
  const t = checker.getWidenedType(checker.getTypeAtLocation(expr));
  if (checker.getIndexInfosOfType(t).length > 0) return { loose: true, props: [] };
  const props: PropInfo[] = [];
  for (const p of checker.getPropertiesOfType(t)) {
    const pt = checker.getWidenedType(checker.getTypeOfSymbolAtLocation(p, expr));
    props.push({
      name: p.getName(),
      type: typeText(checker, pt),
      optional: !!(p.flags & ts.SymbolFlags.Optional),
    });
  }
  return { loose: false, props };
}

function harvest(program: ts.Program): HarvestResult {
  const checker = program.getTypeChecker();
  const events = new Map<string, EventEntry>();
  const dynamic: DynamicEntry[] = [];

  const record = (names: string[], shape: Shape | null | undefined, site: string): void => {
    for (const name of names) {
      let entry = events.get(name);
      if (!entry) {
        entry = { props: new Map(), sites: [], shapeCount: 0 };
        events.set(name, entry);
      }
      entry.sites.push(site);
      if (!shape || shape.loose) continue;
      entry.shapeCount += 1;
      for (const p of shape.props) {
        let acc = entry.props.get(p.name);
        if (!acc) {
          acc = { types: new Set(), present: 0, optional: false };
          entry.props.set(p.name, acc);
        }
        acc.types.add(p.type);
        acc.present += 1;
        if (p.optional) acc.optional = true;
      }
    }
  };

  for (const sf of program.getSourceFiles()) {
    const file = sf.fileName;
    if (!file.startsWith(APP + path.sep)) continue;
    if (/\.test\.tsx?$/.test(file)) continue;
    if (file.startsWith(EVENTS_DIR + path.sep) || file === EVENTS_ENTRY) continue;
    const rel = path.relative(SITES, file);

    const visit = (node: ts.Node): void => {
      ts.forEachChild(node, visit);
      if (!ts.isCallExpression(node)) return;
      const name = calleeName(node);
      if (!name) return;
      const sigType = checker.getTypeAtLocation(node.expression);
      const sigs = sigType.getCallSignatures();
      if (sigs.length !== 1) return;
      const sig = sigs[0]!;
      const { line } = sf.getLineAndCharacterOfPosition(node.getStart());
      const site = `${rel}:${line + 1}`;

      if (name === "trackExposure" && isTrackContextParam(checker, sig, 0)) {
        record(
          ["experiment_exposed"],
          {
            loose: false,
            props: [
              { name: "exp_key", type: "string", optional: true },
              { name: "variant", type: "string", optional: true },
            ],
          },
          site,
        );
        return;
      }
      if (name === "trackOperator" && isTrackContextParam(checker, sig, 3)) {
        if (isForwardedParam(checker, node.arguments[0]!)) return;
        const names =
          node.arguments[0] &&
          !isCastExpr(node.arguments[0]) &&
          stringLiterals(checker, node.arguments[0]);
        if (!names) {
          dynamic.push({ site, callee: name });
          return;
        }
        const shape = node.arguments[2] && propsShape(checker, node.arguments[2]);
        if (shape && !shape.loose && node.arguments[1]) {
          shape.props.unshift({
            name: "target",
            type: typeText(checker, checker.getTypeAtLocation(node.arguments[1])),
            optional: false,
          });
        }
        record(names, shape, site);
        return;
      }
      if (sig.parameters.length < 3 || !isTrackContextParam(checker, sig, 2)) return;
      if (isForwardedParam(checker, node.arguments[0]!)) return;
      const names =
        node.arguments[0] &&
        !isCastExpr(node.arguments[0]) &&
        stringLiterals(checker, node.arguments[0]);
      if (names && name === "trackCreatorFunnel") {
        const shape = node.arguments[1] && propsShape(checker, node.arguments[1]);
        if (shape && !shape.loose) {
          shape.props.push({ name: "funnel_step", type: "number", optional: false });
        }
        record(names, shape, site);
        return;
      }
      if (!names) {
        const t = node.arguments[0]
          ? checker.typeToString(checker.getTypeAtLocation(node.arguments[0]))
          : "?";
        dynamic.push({ site, callee: name, argType: t });
        return;
      }
      const shape = node.arguments[1] && propsShape(checker, node.arguments[1]);
      record(names, shape, site);
    };
    visit(sf);
  }
  return { events, dynamic };
}

function normalizeUnion(typeStrings: Iterable<string>): { type: string; optional: boolean } {
  const tokens = new Set<string>();
  let optional = false;
  for (const s of typeStrings) {
    const parts = /[<(]/.test(s) ? [s] : s.split(" | ");
    for (const p of parts) tokens.add(p);
  }
  if (tokens.delete("undefined")) optional = true;
  if (tokens.has("string")) {
    for (const t of [...tokens]) if (/^"/.test(t)) tokens.delete(t);
  }
  if (tokens.has("number")) {
    for (const t of [...tokens]) if (/^-?\d/.test(t)) tokens.delete(t);
  }
  if (tokens.has("boolean") || (tokens.has("true") && tokens.has("false"))) {
    tokens.delete("true");
    tokens.delete("false");
    tokens.add("boolean");
  }
  return { type: [...tokens].sort().join(" | ") || "undefined", optional };
}

function mergedProps(entry: EventEntry): (PropInfo & { typeCount: number })[] {
  const out: (PropInfo & { typeCount: number })[] = [];
  for (const [name, acc] of [...entry.props.entries()].sort((a, b) =>
    a[0].localeCompare(b[0]),
  )) {
    const norm = normalizeUnion(acc.types);
    out.push({
      name,
      type: norm.type,
      optional: norm.optional || acc.optional || acc.present < entry.shapeCount,
      typeCount: acc.types.size,
    });
  }
  return out;
}

function report({ events, dynamic }: HarvestResult): void {
  const out = { eventCount: events.size, dynamicCount: dynamic.length, events: {} as Record<string, unknown>, dynamic };
  for (const name of [...events.keys()].sort()) {
    const entry = events.get(name)!;
    out.events[name] = {
      sites: entry.sites,
      shapeCount: entry.shapeCount,
      props: mergedProps(entry),
    };
  }
  process.stdout.write(JSON.stringify(out, null, 2) + "\n");
}

function draft({ events }: HarvestResult): void {
  const lines: string[] = [];
  for (const name of [...events.keys()].sort()) {
    const entry = events.get(name)!;
    const props = mergedProps(entry);
    if (props.length === 0) {
      lines.push(`  ${JSON.stringify(name)}: Record<string, never>;`);
      continue;
    }
    lines.push(`  ${JSON.stringify(name)}: {`);
    for (const p of props) {
      const key = /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(p.name) ? p.name : JSON.stringify(p.name);
      lines.push(`    ${key}${p.optional ? "?" : ""}: ${p.type};`);
    }
    lines.push("  };");
  }
  process.stdout.write(`export type TelemetryEvents = {\n${lines.join("\n")}\n};\n`);
}

function eventEntryLines(name: string, entry: EventEntry, indent: string): string[] {
  const props = mergedProps(entry);
  const key = /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) ? name : JSON.stringify(name);
  if (props.length === 0) return [`${indent}${key}: Record<string, never>;`];
  const lines = [`${indent}${key}: {`];
  for (const p of props) {
    const pk = /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(p.name) ? p.name : JSON.stringify(p.name);
    lines.push(`${indent}  ${pk}${p.optional ? "?" : ""}: ${p.type};`);
  }
  lines.push(`${indent}};`);
  return lines;
}

function write({ events }: HarvestResult): void {
  const grouped = new Map<string, string[]>(SURFACES.map(([slug]) => [slug, []]));
  for (const name of [...events.keys()].sort()) {
    const found = SURFACES.find(([, , re]) => re.test(name))!;
    grouped.get(found[0])!.push(name);
  }
  fs.mkdirSync(EVENTS_DIR, { recursive: true });
  for (const [slug, typeName] of SURFACES) {
    const lines = [`export type ${typeName} = {`];
    for (const name of grouped.get(slug)!) {
      lines.push(...eventEntryLines(name, events.get(name)!, "  "));
    }
    lines.push("};");
    fs.writeFileSync(path.join(EVENTS_DIR, `${slug}.ts`), lines.join("\n") + "\n");
    console.log(`wrote events/${slug}.ts (${grouped.get(slug)!.length} events)`);
  }
  const entry: string[] = [];
  for (const [slug, typeName] of SURFACES) {
    entry.push(`import type { ${typeName} } from "./events/${slug}";`);
  }
  entry.push("");
  entry.push(
    `export type TelemetryEvents = ${SURFACES.map(([, t]) => t).join(" &\n  ")};`,
  );
  entry.push("");
  entry.push("export type TelemetryEventName = keyof TelemetryEvents;");
  fs.writeFileSync(EVENTS_ENTRY, entry.join("\n") + "\n");
  console.log(`wrote events.ts (${events.size} events)`);
}

function catalogKeys(program: ts.Program): Set<string> {
  const keys = new Set<string>();
  for (const sf of program.getSourceFiles()) {
    const file = sf.fileName;
    if (file !== EVENTS_ENTRY && !file.startsWith(EVENTS_DIR + path.sep)) continue;
    const visit = (node: ts.Node): void => {
      ts.forEachChild(node, visit);
      if (!ts.isTypeAliasDeclaration(node) || !/Events$/.test(node.name.text)) return;
      const collect = (typeNode: ts.TypeNode): void => {
        if (ts.isTypeLiteralNode(typeNode)) {
          for (const m of typeNode.members) {
            if (!ts.isPropertySignature(m) || !m.name) continue;
            if (ts.isStringLiteral(m.name) || ts.isIdentifier(m.name)) {
              keys.add(m.name.text);
            }
          }
        } else if (ts.isIntersectionTypeNode(typeNode)) {
          typeNode.types.forEach(collect);
        }
      };
      collect(node.type);
    };
    visit(sf);
  }
  return keys;
}

function check(program: ts.Program, { events, dynamic }: HarvestResult): number {
  const cataloged = catalogKeys(program);
  const used = new Set(events.keys());
  const missing = [...used].filter((k) => !cataloged.has(k)).sort();
  const stale = [...cataloged].filter((k) => !used.has(k)).sort();
  if (missing.length === 0 && stale.length === 0 && dynamic.length === 0) {
    console.log(`telemetry catalog in sync: ${used.size} events`);
    return 0;
  }
  for (const k of missing) {
    console.error(
      `uncataloged event: ${k} (${events.get(k)!.sites.slice(0, 3).join(", ")})`,
    );
  }
  for (const k of stale) console.error(`cataloged but never emitted: ${k}`);
  for (const d of dynamic) {
    console.error(`non-literal event name at ${d.site} (${d.argType ?? d.callee})`);
  }
  console.error(
    "fix: update catalyrst/sites/packages/core/src/lib/telemetry/events.ts (draft: node scripts/telemetry-catalog.mts --draft)",
  );
  return 1;
}

// Machine-readable event contract: events.ts is compile-time-only and erased at
// runtime, so this emits a checked-in JSON contract \u{2014} event names + per-prop
// structured kinds/enums \u{2014} that runtime consumers read instead (metric-ref
// gate, track.ts's emit-path validator, catalyrst-telemetry's ingest validator).
// Generated from the SAME harvest as events.ts, so the two never drift;
// `--check` verifies it is in sync.

type ContractProp = { kind: string; values?: (string | number)[]; optional: boolean };
type ContractEvent = { loose: boolean; props: Record<string, ContractProp> };

function classifyType(typeStr: string): { kind: string; values?: (string | number)[] } {
  const t = typeStr.trim();
  if (t === "string") return { kind: "string" };
  if (t === "number") return { kind: "number" };
  if (t === "boolean") return { kind: "boolean" };
  // Anything with a generic/object/tuple bracket is not a flat literal union.
  const tokens = /[<({[]/.test(t) ? [t] : t.split(" | ").map((s) => s.trim());
  const strVals: string[] = [];
  const numVals: number[] = [];
  let allStr = true;
  let allNum = true;
  for (const tok of tokens) {
    if (/^".*"$/.test(tok)) {
      allNum = false;
      try {
        strVals.push(JSON.parse(tok) as string);
      } catch {
        allStr = false;
      }
    } else if (/^-?\d+(\.\d+)?$/.test(tok)) {
      allStr = false;
      numVals.push(Number(tok));
    } else {
      allStr = false;
      allNum = false;
    }
  }
  if (allStr && strVals.length) return { kind: "enum-string", values: strVals.sort() };
  if (allNum && numVals.length) return { kind: "enum-number", values: numVals.sort((a, b) => a - b) };
  return { kind: "unknown" };
}

function buildContract({ events }: HarvestResult): {
  version: number;
  contextProps: Record<string, ContractProp>;
  events: Record<string, ContractEvent>;
} {
  const evOut: Record<string, ContractEvent> = {};
  for (const name of [...events.keys()].sort()) {
    const entry = events.get(name)!;
    const props: Record<string, ContractProp> = {};
    for (const p of mergedProps(entry)) {
      const c = classifyType(p.type);
      props[p.name] = {
        kind: c.kind,
        ...(c.values ? { values: c.values } : {}),
        optional: p.optional,
      };
    }
    // shapeCount === 0 means no call site ever gave a typed props object (a
    // forwarded/loose arg): don't over-validate \u{2014} accept any props.
    evOut[name] = { loose: entry.shapeCount === 0, props };
  }
  return {
    version: 1,
    // story/variant/exp_key are injected into every event's `properties` by
    // buildSegmentBody (track.ts); validators always allow them.
    contextProps: {
      exp_key: { kind: "string", optional: true },
      story: { kind: "string", optional: true },
      variant: { kind: "string", optional: true },
    },
    events: evOut,
  };
}

function contractJson(data: HarvestResult): string {
  return JSON.stringify(buildContract(data), null, 2) + "\n";
}

function contractWrite(data: HarvestResult): void {
  fs.writeFileSync(CONTRACT_PATH, contractJson(data));
  console.log(
    `wrote ${path.relative(SITES, CONTRACT_PATH)} (${data.events.size} events)`,
  );
}

function contractCheck(data: HarvestResult): number {
  const want = contractJson(data);
  const have = fs.existsSync(CONTRACT_PATH) ? fs.readFileSync(CONTRACT_PATH, "utf8") : "";
  if (want === have) {
    console.log(`telemetry contract in sync: ${data.events.size} events`);
    return 0;
  }
  console.error(
    "telemetry contract out of date \u{2014} run: node scripts/telemetry-catalog.mts --contract",
  );
  return 1;
}

const program = loadProgram();
const data = harvest(program);
if (mode === "report") report(data);
else if (mode === "draft") draft(data);
else if (mode === "write") {
  write(data);
  contractWrite(data);
} else if (mode === "contract") contractWrite(data);
else {
  const nameStatus = check(program, data);
  const contractStatus = contractCheck(data);
  process.exit(nameStatus || contractStatus);
}
