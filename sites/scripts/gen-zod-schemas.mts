import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import type * as TSNamespace from "typescript";

const require = createRequire(import.meta.url);
const ts: typeof TSNamespace = require("typescript");

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

function crateDirs(): string[] {
  return readdirSync(SRC_ROOT, { withFileTypes: true })
    .filter((e) => e.isDirectory() && !SKIP_DIRS.has(e.name) && EMIT_CRATES.has(e.name))
    .map((e) => e.name)
    .sort();
}

function crateAliases(crate: string): Alias[] {
  const dir = join(SRC_ROOT, crate);
  const aliases: Alias[] = [];
  for (const entry of readdirSync(dir).sort()) {
    if (!entry.endsWith(".ts")) continue;
    const file = join(dir, entry);
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
        fileBase: entry.replace(/\.ts$/, ""),
        rel: `${crate}/${entry}`,
        params: new Map(),
        deps: new Set(),
        body: "",
      });
    }
  }
  return aliases;
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
    case ts.SyntaxKind.BigIntKeyword:
      return "z.bigint()";
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
  const aliases = crateAliases(crate);
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

  const imports = [...aliases]
    .sort((x, y) => x.name.localeCompare(y.name))
    .map(
      (a) =>
        `import type { ${a.name} } from "@ui/generated/catalyst/${crate}/${a.fileBase}";`,
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
    `// GENERATED from catalyrst/ui3/src/generated/catalyst/${crate} by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.`,
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

  if (fresh.size === 0) {
    fail(
      "no schema modules generated from catalyrst/ui3/src/generated/catalyst — a run that inspected nothing must not report success, and --write here would delete every committed module",
    );
  }

  if (check) {
    const committed = currentOutputs();
    const drifted: string[] = [];
    for (const [name, content] of fresh) {
      if (committed.get(name) !== content) drifted.push(name);
    }
    for (const name of committed.keys()) {
      if (!fresh.has(name)) drifted.push(`${name} (stale)`);
    }
    if (drifted.length > 0) {
      console.error("gen-zod-schemas: generated zod schemas drifted from the ts-rs bindings:");
      for (const d of drifted) console.error(`  packages/data/src/lib/catalyst/generated-schemas/${d}`);
      console.error("fix: cd catalyrst/sites && npm run gen:schemas   # then commit the output");
      process.exit(1);
    }
    console.log(`gen-zod-schemas: ${fresh.size} schema modules up to date`);
    return;
  }

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
