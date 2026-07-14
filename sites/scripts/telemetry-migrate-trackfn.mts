import ts from "typescript6";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SITES = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const APP = path.join(SITES, "packages");
const TELEMETRY = path.join(APP, "core", "src", "lib", "telemetry");

const CANONICAL =
  "(event:string,props:Record<string,unknown>,ctx:TrackContext)=>void";

function* walk(dir: string): Generator<string> {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(full);
    else if (/\.tsx?$/.test(entry.name)) yield full;
  }
}

let changed = 0;
for (const file of walk(APP)) {
  if (file.startsWith(TELEMETRY + path.sep)) continue;
  const text = fs.readFileSync(file, "utf8");
  if (!text.includes("TrackFn")) continue;
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);

  let alias: ts.TypeAliasDeclaration | null = null;
  let trackImport: ts.ImportDeclaration | null = null;
  for (const stmt of sf.statements) {
    if (
      ts.isTypeAliasDeclaration(stmt) &&
      stmt.name.text === "TrackFn" &&
      stmt.type.getText(sf).replace(/\s+/g, "").replace(",)", ")") === CANONICAL
    ) {
      alias = stmt;
    }
    if (
      ts.isImportDeclaration(stmt) &&
      ts.isStringLiteral(stmt.moduleSpecifier) &&
      /\/telemetry\/track$/.test(stmt.moduleSpecifier.text) &&
      stmt.importClause?.namedBindings &&
      ts.isNamedImports(stmt.importClause.namedBindings)
    ) {
      trackImport = stmt;
    }
  }
  if (!alias || !trackImport) continue;

  const importClause = trackImport.importClause;
  const namedBindings = importClause?.namedBindings;
  if (!namedBindings || !ts.isNamedImports(namedBindings)) continue;
  const imports = namedBindings;
  const hasTrackFn = imports.elements.some((e) => e.name.text === "TrackFn");
  const exported = alias.modifiers?.some(
    (m) => m.kind === ts.SyntaxKind.ExportKeyword,
  );

  const edits: { pos: number; end: number; text: string }[] = [];
  if (!hasTrackFn) {
    const last = imports.elements[imports.elements.length - 1]!;
    edits.push({ pos: last.getEnd(), end: last.getEnd(), text: ", type TrackFn" });
  }
  const aliasStart = alias.getStart(sf);
  let aliasEnd = alias.getEnd();
  if (text[aliasEnd] === "\n") {
    aliasEnd += 1;
    if (!exported && text[aliasEnd] === "\n") aliasEnd += 1;
  }
  edits.push({
    pos: aliasStart,
    end: aliasEnd,
    text: exported ? "export type { TrackFn };\n" : "",
  });

  edits.sort((a, b) => b.pos - a.pos);
  let out = text;
  for (const e of edits) out = out.slice(0, e.pos) + e.text + out.slice(e.end);
  fs.writeFileSync(file, out);
  changed += 1;
  console.log(path.relative(SITES, file));
}
console.log(`rewrote ${changed} files`);
