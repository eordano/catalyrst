#!/usr/bin/env node
// codemod-story-id-types.mts -- one-off migration: type `const STORY = "id";`
// declarations as `StoryId` wherever the value already matches a real spec
// (catalyrst/sites/packages/features/src/stories/**/story.md). Run once, review the diff, commit script
// + result together. Values that don't match a spec are left untouched --
// they need a product decision, not a mechanical rewrite.
import ts from "typescript6";
import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SITES = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const APP = path.join(SITES, "packages");
const STORY_ID_MODULE = path.join(APP, "core", "src", "lib", "telemetry", "story-id.ts");

const validIds = new Set(
  [...readFileSync(STORY_ID_MODULE, "utf8").matchAll(/\| "([^"]+)"/g)].map((m) => m[1]!),
);

function allSourceFiles(dir: string, acc: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules") continue;
    const p = path.join(dir, name);
    if (statSync(p).isDirectory()) allSourceFiles(p, acc);
    else if (/\.tsx?$/.test(name) && p !== STORY_ID_MODULE) acc.push(p);
  }
  return acc;
}

function importSpecifier(fromFile: string): string {
  const rel = path.relative(path.dirname(fromFile), STORY_ID_MODULE).replace(/\.ts$/, "");
  return rel.startsWith(".") ? rel : `./${rel}`;
}

let migrated = 0;
let skipped = 0;
for (const file of allSourceFiles(APP)) {
  const text = readFileSync(file, "utf8");
  if (!/^const STORY = "/m.test(text)) continue;
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);

  let storyNode: ts.VariableDeclaration | null = null;
  let lastImportEnd = -1;
  let hasStoryIdImport = false;
  for (const stmt of sf.statements) {
    if (ts.isImportDeclaration(stmt)) {
      lastImportEnd = stmt.end;
      if (
        stmt.moduleSpecifier.getText(sf).replace(/['"]/g, "") ===
        importSpecifier(file)
      ) {
        hasStoryIdImport = true;
      }
      continue;
    }
    if (
      ts.isVariableStatement(stmt) &&
      stmt.declarationList.declarations.length === 1
    ) {
      const decl = stmt.declarationList.declarations[0];
      if (
        ts.isIdentifier(decl.name) &&
        decl.name.text === "STORY" &&
        !decl.type &&
        decl.initializer &&
        ts.isStringLiteral(decl.initializer)
      ) {
        storyNode = decl;
      }
    }
  }

  if (!storyNode) continue;
  const init = storyNode.initializer;
  if (!init || !ts.isStringLiteral(init)) continue;
  const value = init.text;
  if (!validIds.has(value)) {
    skipped += 1;
    continue;
  }

  let out = text.slice(0, storyNode.name.end) + ": StoryId" + text.slice(storyNode.name.end);
  if (!hasStoryIdImport) {
    const importLine = `import type { StoryId } from "${importSpecifier(file)}";\n`;
    const insertAt = lastImportEnd >= 0 ? lastImportEnd + 1 : 0;
    out = out.slice(0, insertAt) + importLine + out.slice(insertAt);
  }
  writeFileSync(file, out);
  migrated += 1;
}

console.log(`codemod-story-id-types: migrated ${migrated} files, skipped ${skipped} (no matching spec)`);
