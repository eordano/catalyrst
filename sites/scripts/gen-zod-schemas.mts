import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import type * as TSNamespace from "typescript6";

const require = createRequire(import.meta.url);
const ts: typeof TSNamespace = require("typescript6");

const SITES = fileURLToPath(new URL("..", import.meta.url));
const SRC_ROOT = join(SITES, "..", "ui3", "src", "generated", "catalyst");
const OUT_ROOT = join(SITES, "packages", "data", "src", "lib", "catalyst", "generated-schemas");
const SKIP_DIRS = new Set(["openapi"]);
// Only crates whose schemas have importers outside generated-schemas/ are
// emitted; add a crate here when its first external import appears.
const EMIT_CRATES = new Set([
  "builder",
  "camera-reel",
  "comms",
  "communities",
  "credits",
  "economy",
  "events",
  "governance",
  "map",
  "market",
  "notifications",
  "places",
  "presence",
  "world-storage",
  "worlds",
]);
const IDENT_RE = /^[A-Za-z_$][A-Za-z0-9_$]*$/;

type Alias = {
  name: string;
  decl: TSNamespace.TypeAliasDeclaration;
  sf: TSNamespace.SourceFile;
  fileBase: string;
  rel: string;
  params: Map<string, string>;
  deps: Set<string>;
  body: string;
};

type Ctx = {
  rel: string;
  sf: TSNamespace.SourceFile;
  params: Map<string, string>;
  deps: Set<string>;
  known: Set<string>;
};

function fail(msg: string): never {
  console.error(`gen-zod-schemas: ${msg}`);
  process.exit(1);
}

// The bridge is the second boundary this generator serves. Its types come from
// the same ts-rs export as the catalyst ones -- gen-ts-types.sh runs it against
// bevy-explorer -- so the schemas are generated, not hand-written, and the
// existing types-bridge drift lane already keeps the input honest.
//
// Two things differ. Its directory is flat rather than one dir per crate, so it
// emits a single module; and it is consumed from inside ui3, which has no
// `@ui/*` alias to itself, so its imports are relative.
const BRIDGE_SRC = join(SITES, "..", "ui3", "src", "generated", "bridge");
const BRIDGE_OUT = join(SITES, "..", "ui3", "src", "generated", "bridge-schemas.ts");

// The editor bus is the third boundary, and like the bridge it is ts-rs output
// consumed from inside ui3, so the same emitter serves it with relative imports.
// It differs in one way that matters: gen-ts-types.sh assembles the per-type
// ts-rs files into a single editor-bus.ts and then deletes the directory, so
// there is no directory to walk -- the source here is one file.
const EDITOR_SRC = join(SITES, "..", "ui3", "src", "generated", "editor-bus.ts");
const EDITOR_OUT = join(SITES, "..", "ui3", "src", "generated", "editor-bus-schemas.ts");

function crateDirs(): string[] {
  return readdirSync(SRC_ROOT, { withFileTypes: true })
    .filter((e) => e.isDirectory() && !SKIP_DIRS.has(e.name) && EMIT_CRATES.has(e.name))
    .map((e) => e.name)
    .sort();
}

// Split out from aliasesIn because not every source is a directory: ts-rs emits
// one file per type, but gen-ts-types.sh assembles the editor-bus types into a
// single file and deletes the per-type directory, so that target has to be read
// a file at a time.
function aliasesInFile(file: string, rel: string, fileBase: string): Alias[] {
  const aliases: Alias[] = [];
  const sf = ts.createSourceFile(
    file,
    readFileSync(file, "utf8"),
    ts.ScriptTarget.Latest,
    true,
  );
  for (const stmt of sf.statements) {
    if (!ts.isTypeAliasDeclaration(stmt)) continue;
    aliases.push({
      name: stmt.name.text,
      decl: stmt,
      sf,
      fileBase,
      rel,
      params: new Map(),
      deps: new Set(),
      body: "",
    });
  }
  return aliases;
}

function aliasesIn(dir: string, relPrefix: string): Alias[] {
  const aliases: Alias[] = [];
  for (const entry of readdirSync(dir).sort()) {
    if (!entry.endsWith(".ts")) continue;
    aliases.push(
      ...aliasesInFile(join(dir, entry), `${relPrefix}${entry}`, entry.replace(/\.ts$/, "")),
    );
  }
  return aliases;
}

function crateAliases(crate: string): Alias[] {
  return aliasesIn(join(SRC_ROOT, crate), `${crate}/`);
}

function isRecordRef(node: TSNamespace.TypeNode): node is TSNamespace.TypeReferenceNode {
  return (
    ts.isTypeReferenceNode(node) &&
    ts.isIdentifier(node.typeName) &&
    node.typeName.text === "Record"
  );
}

function zodOf(node: TSNamespace.TypeNode, ctx: Ctx, depth: number): string {
  switch (node.kind) {
    case ts.SyntaxKind.StringKeyword:
      return "z.string()";
    case ts.SyntaxKind.NumberKeyword:
      return "z.number()";
    case ts.SyntaxKind.BooleanKeyword:
      return "z.boolean()";
    // ts-rs maps Rust's u64/i64 to `bigint`, but nothing on the wire is a
    // bigint: JSON has one number type, and both the engine bridge and the HTTP
    // services send these as plain numbers. A `z.bigint()` therefore rejects
    // every payload carrying one -- which it did, silently, for every friends
    // push with a pending request. Coercing accepts the number (and a numeric
    // string) while still INFERRING bigint, so the mutual-assignability assert
    // against the ts-rs type still holds.
    case ts.SyntaxKind.BigIntKeyword:
      return "z.coerce.bigint()";
    case ts.SyntaxKind.UnknownKeyword:
      return "z.unknown()";
    case ts.SyntaxKind.NeverKeyword:
      return "z.never()";
  }
  if (ts.isParenthesizedTypeNode(node)) return zodOf(node.type, ctx, depth);
  if (ts.isLiteralTypeNode(node)) {
    const lit = node.literal;
    if (lit.kind === ts.SyntaxKind.NullKeyword) return "z.null()";
    if (lit.kind === ts.SyntaxKind.TrueKeyword) return "z.literal(true)";
    if (lit.kind === ts.SyntaxKind.FalseKeyword) return "z.literal(false)";
    if (ts.isStringLiteral(lit)) return `z.literal(${JSON.stringify(lit.text)})`;
    if (ts.isNumericLiteral(lit)) return `z.literal(${lit.text})`;
    fail(`unsupported literal in ${ctx.rel}: ${lit.getText(ctx.sf)}`);
  }
  if (ts.isArrayTypeNode(node)) {
    return `z.array(${zodOf(node.elementType, ctx, depth)})`;
  }
  if (ts.isTupleTypeNode(node)) {
    const parts = node.elements.map((e) => zodOf(e, ctx, depth));
    return `z.tuple([${parts.join(", ")}])`;
  }
  if (ts.isUnionTypeNode(node)) return unionOf(node.types, ctx, depth);
  if (ts.isIntersectionTypeNode(node)) {
    const [base, rec] = node.types;
    if (
      node.types.length === 2 &&
      base &&
      rec &&
      ts.isTypeLiteralNode(base) &&
      isRecordRef(rec) &&
      rec.typeArguments?.length === 2 &&
      rec.typeArguments[0]!.kind === ts.SyntaxKind.StringKeyword &&
      rec.typeArguments[1]!.kind === ts.SyntaxKind.UnknownKeyword
    ) {
      return `${objectOf(base, ctx, depth)}.passthrough()`;
    }
    fail(`unsupported intersection in ${ctx.rel}: ${node.getText(ctx.sf)}`);
  }
  if (ts.isTypeLiteralNode(node)) return objectOf(node, ctx, depth);
  if (ts.isMappedTypeNode(node)) {
    const constraint = node.typeParameter.constraint;
    if (!constraint || constraint.kind !== ts.SyntaxKind.StringKeyword) {
      fail(`unsupported mapped type in ${ctx.rel}: ${node.getText(ctx.sf)}`);
    }
    if (!node.type) fail(`unsupported mapped type (no value type) in ${ctx.rel}: ${node.getText(ctx.sf)}`);
    const value = zodOf(node.type, ctx, depth);
    return node.questionToken
      ? `z.partialRecord(z.string(), ${value})`
      : `z.record(z.string(), ${value})`;
  }
  if (ts.isTypeReferenceNode(node)) {
    if (!ts.isIdentifier(node.typeName)) {
      fail(`unsupported qualified reference in ${ctx.rel}: ${node.getText(ctx.sf)}`);
    }
    const name = node.typeName.text;
    const args = node.typeArguments ?? [];
    if (name === "Array") {
      return `z.array(${zodOf(args[0]!, ctx, depth)})`;
    }
    if (name === "Record") {
      if (args[0]!.kind !== ts.SyntaxKind.StringKeyword) {
        fail(`unsupported Record key in ${ctx.rel}: ${node.getText(ctx.sf)}`);
      }
      return `z.record(z.string(), ${zodOf(args[1]!, ctx, depth)})`;
    }
    if (ctx.params.has(name)) return ctx.params.get(name)!;
    if (ctx.known.has(name)) {
      if (args.length > 0) {
        const parts = args.map((a) => zodOf(a, ctx, depth));
        ctx.deps.add(name);
        return `${name}Schema(${parts.join(", ")})`;
      }
      ctx.deps.add(name);
      return `${name}Schema`;
    }
    fail(`unknown type reference "${name}" in ${ctx.rel}`);
  }
  fail(`unsupported type node ${ts.SyntaxKind[node.kind]} in ${ctx.rel}`);
}

function unionOf(members: readonly TSNamespace.TypeNode[], ctx: Ctx, depth: number): string {
  let nullable = false;
  const rest: TSNamespace.TypeNode[] = [];
  for (const m of members) {
    if (ts.isLiteralTypeNode(m) && m.literal.kind === ts.SyntaxKind.NullKeyword) {
      nullable = true;
    } else {
      rest.push(m);
    }
  }
  let expr: string;
  if (rest.length === 0) return "z.null()";
  if (rest.length === 1) {
    expr = zodOf(rest[0]!, ctx, depth);
  } else if (
    rest.every((m) => ts.isLiteralTypeNode(m) && ts.isStringLiteral(m.literal))
  ) {
    const values = rest.map((m) => {
      const lit = (m as TSNamespace.LiteralTypeNode).literal as TSNamespace.StringLiteral;
      return JSON.stringify(lit.text);
    });
    expr = `z.enum([${values.join(", ")}])`;
  } else {
    const parts = rest.map((m) => zodOf(m, ctx, depth));
    expr = `z.union([${parts.join(", ")}])`;
  }
  return nullable ? `${expr}.nullable()` : expr;
}

function objectOf(node: TSNamespace.TypeLiteralNode, ctx: Ctx, depth: number): string {
  if (node.members.length === 0) return "z.object({})";
  const pad = "  ".repeat(depth + 1);
  const fields = node.members.map((m) => {
    if (!ts.isPropertySignature(m) || !m.type) {
      fail(`unsupported member in ${ctx.rel}: ${m.getText(ctx.sf)}`);
    }
    if (!ts.isIdentifier(m.name) && !ts.isStringLiteral(m.name)) {
      fail(`unsupported member name in ${ctx.rel}: ${m.getText(ctx.sf)}`);
    }
    const name = m.name.text;
    const key = IDENT_RE.test(name) ? name : JSON.stringify(name);
    let expr = zodOf(m.type, ctx, depth + 1);
    if (m.questionToken) expr += ".optional()";
    return `${pad}${key}: ${expr},`;
  });
  return `z.object({\n${fields.join("\n")}\n${"  ".repeat(depth)}})`;
}

function topoSort(aliases: Alias[]): Alias[] {
  const byName = new Map(aliases.map((a) => [a.name, a]));
  const order: Alias[] = [];
  const state = new Map<string, "visiting" | "done">();
  const visit = (name: string, chain: string[]): void => {
    if (state.get(name) === "done") return;
    if (state.get(name) === "visiting") {
      fail(`schema dependency cycle: ${[...chain, name].join(" -> ")}`);
    }
    state.set(name, "visiting");
    const a = byName.get(name)!;
    for (const dep of [...a.deps].sort()) visit(dep, [...chain, name]);
    state.set(name, "done");
    order.push(a);
  };
  for (const a of aliases.map((x) => x.name).sort()) visit(a, []);
  return order;
}

function emitCrate(crate: string): string | null {
  return emitAliases(
    crateAliases(crate),
    (a) => `@ui/generated/catalyst/${crate}/${a.fileBase}`,
    `catalyrst/ui3/src/generated/catalyst/${crate}`,
  );
}

function emitAliases(
  aliases: Alias[],
  importOf: (a: Alias) => string,
  provenance: string,
): string | null {
  if (aliases.length === 0) return null;
  const known = new Set(aliases.map((a) => a.name));

  for (const a of aliases) {
    const params = new Map<string, string>();
    for (const tp of a.decl.typeParameters ?? []) {
      params.set(tp.name.text, tp.name.text.toLowerCase());
    }
    a.params = params;
    a.deps = new Set();
    const ctx: Ctx = { rel: a.rel, sf: a.sf, params, deps: a.deps, known };
    a.body = zodOf(a.decl.type, ctx, 0);
  }

  const ordered = topoSort(aliases);

  // Grouped by module: the per-crate targets are one type per file, so this is
  // a no-op for them, but a single-file target like the editor bus would
  // otherwise emit thirteen `import type` lines from the same specifier.
  const byModule = new Map<string, string[]>();
  for (const a of [...aliases].sort((x, y) => x.name.localeCompare(y.name))) {
    const mod = importOf(a);
    const names = byModule.get(mod);
    if (names) names.push(a.name);
    else byModule.set(mod, [a.name]);
  }
  const imports = [...byModule].map(
    ([mod, names]) => `import type { ${names.join(", ")} } from "${mod}";`,
  );

  const decls = ordered.map((a) => {
    if (a.params.size === 0) {
      return `export const ${a.name}Schema = ${a.body};`;
    }
    const tps = [...a.params.keys()]
      .map((p) => `${p} extends z.ZodType`)
      .join(", ");
    const args = [...a.params.entries()]
      .map(([P, p]) => `${p}: ${P}`)
      .join(", ");
    return `export const ${a.name}Schema = <${tps}>(${args}) =>\n  ${a.body.replaceAll("\n", "\n  ")};`;
  });

  const asserts = [...aliases]
    .sort((x, y) => x.name.localeCompare(y.name))
    .map((a) => {
      if (a.params.size === 0) {
        return `export type _Assert${a.name} = Assert<Mutual<${a.name}, z.infer<typeof ${a.name}Schema>>>;`;
      }
      const rs = `${a.name}<${[...a.params.keys()].map(() => "unknown").join(", ")}>`;
      const inst = `${a.name}Schema<${[...a.params.keys()].map(() => "z.ZodUnknown").join(", ")}>`;
      return `export type _Assert${a.name} = Assert<Mutual<${rs}, z.infer<ReturnType<typeof ${inst}>>>>;`;
    });

  return [
    `// GENERATED from ${provenance} by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.`,
    `import { z } from "zod";`,
    "",
    imports.join("\n"),
    "",
    decls.join("\n\n"),
    "",
    "type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;",
    "type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;",
    "type Assert<T extends true> = T;",
    "",
    asserts.join("\n"),
    "",
  ].join("\n");
}

function generateAll(): Map<string, string> {
  const files = new Map<string, string>();
  for (const crate of crateDirs()) {
    const content = emitCrate(crate);
    if (content != null) files.set(`${crate}.ts`, content);
  }
  return files;
}

// The bridge emits one module rather than one per crate, and lands in ui3
// rather than in sites, so it is tracked as its own single-file target instead
// of being folded into the catalyst map.
function generateBridge(): string {
  const content = emitAliases(
    aliasesIn(BRIDGE_SRC, ""),
    (a) => `./bridge/${a.fileBase}`,
    "catalyrst/ui3/src/generated/bridge",
  );
  if (content == null) {
    fail(
      "no type aliases found in catalyrst/ui3/src/generated/bridge \u2014 a run that inspected nothing must not report success",
    );
  }
  return content;
}

function generateEditorBus(): string {
  const content = emitAliases(
    aliasesInFile(EDITOR_SRC, "editor-bus.ts", "editor-bus"),
    () => "./editor-bus",
    "catalyrst/ui3/src/generated/editor-bus.ts",
  );
  if (content == null) {
    fail(
      "no type aliases found in catalyrst/ui3/src/generated/editor-bus.ts \u{2014} a run that inspected nothing must not report success",
    );
  }
  return content;
}

function currentFile(path: string): string | null {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return null;
  }
}

function currentOutputs(): Map<string, string> {
  const files = new Map<string, string>();
  let entries: string[] = [];
  try {
    entries = readdirSync(OUT_ROOT).sort();
  } catch {
    return files;
  }
  for (const entry of entries) {
    if (!entry.endsWith(".ts")) continue;
    files.set(entry, readFileSync(join(OUT_ROOT, entry), "utf8"));
  }
  return files;
}

function main(): void {
  const check = process.argv.includes("--check");
  const fresh = generateAll();
  const freshBridge = generateBridge();
  const freshEditorBus = generateEditorBus();

  if (fresh.size === 0) {
    fail(
      "no schema modules generated from catalyrst/ui3/src/generated/catalyst \u{2014} a run that inspected nothing must not report success, and --write here would delete every committed module",
    );
  }

  if (check) {
    const committed = currentOutputs();
    // One list across all three targets. Reporting the first drift and exiting
    // would let a ui3-side drift hide every catalyst one behind it, so the run
    // would have to be repeated once per drifted module.
    const drifted: string[] = [];
    if (currentFile(BRIDGE_OUT) !== freshBridge) {
      drifted.push("../ui3/src/generated/bridge-schemas.ts");
    }
    if (currentFile(EDITOR_OUT) !== freshEditorBus) {
      drifted.push("../ui3/src/generated/editor-bus-schemas.ts");
    }
    for (const [name, content] of fresh) {
      if (committed.get(name) !== content) {
        drifted.push(`packages/data/src/lib/catalyst/generated-schemas/${name}`);
      }
    }
    for (const name of committed.keys()) {
      if (!fresh.has(name)) {
        drifted.push(`packages/data/src/lib/catalyst/generated-schemas/${name} (stale)`);
      }
    }
    if (drifted.length > 0) {
      console.error("gen-zod-schemas: generated zod schemas drifted from the ts-rs bindings:");
      for (const d of drifted) console.error(`  ${d}`);
      console.error("fix: cd catalyrst/sites && npm run gen:schemas   # then commit the output");
      process.exit(1);
    }
    console.log(
      `gen-zod-schemas: ${fresh.size} catalyst schema modules + the bridge and editor-bus modules up to date`,
    );
    return;
  }

  writeFileSync(BRIDGE_OUT, freshBridge);
  writeFileSync(EDITOR_OUT, freshEditorBus);
  mkdirSync(OUT_ROOT, { recursive: true });
  for (const [name] of currentOutputs()) {
    if (!fresh.has(name)) rmSync(join(OUT_ROOT, name));
  }
  for (const [name, content] of fresh) {
    writeFileSync(join(OUT_ROOT, name), content);
  }
  console.log(
    `gen-zod-schemas: wrote ${fresh.size} schema modules to packages/data/src/lib/catalyst/generated-schemas/`,
  );
}

main();
