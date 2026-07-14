#!/usr/bin/env node
import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const sites = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const manifestFile = join(sites, ".react-router", "types", "+routes.ts");

function routeManifest(): Set<string> {
  const src = readFileSync(manifestFile, "utf8");
  const block = src.match(/type Pages = \{([\s\S]*?)\n\};/);
  if (!block) throw new Error("Pages block not found in " + manifestFile);
  return new Set([...block[1]!.matchAll(/^ {2}"([^"]+)": \{/gm)].map((m) => m[1]!));
}

function* walk(dir: string): Generator<string> {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) yield* walk(p);
    else if (p.endsWith(".tsx")) yield p;
  }
}

const manifest = routeManifest();
const staticRoutes = new Set([...manifest].filter((r) => !/[:*]/.test(r)));
const targets: string[] = [
  join(sites, "packages", "routes", "app", "root.tsx"),
  ...walk(join(sites, "packages", "routes", "app", "routes")),
  ...walk(join(sites, "packages", "features", "src", "components")),
];

const stragglers: [string, string][] = [];
let filesChanged = 0;
let rewrites = 0;

for (const file of targets) {
  const before = readFileSync(file, "utf8");
  if (/\bconst href\b|\blet href\b/.test(before)) {
    if (/to="\//.test(before)) stragglers.push([file, "local `href` binding"]);
    continue;
  }
  let changed = false;
  const after = before.replace(/to="(\/[^"]*)"/g, (whole: string, path: string) => {
    if (path.includes("?") || path.includes("#")) {
      stragglers.push([file, whole]);
      return whole;
    }
    if (!staticRoutes.has(path)) {
      stragglers.push([file, `${whole} (not in route manifest)`]);
      return whole;
    }
    changed = true;
    rewrites += 1;
    return `to={href("${path}")}`;
  });
  if (!changed) continue;
  let out = after;
  if (!/from "[^"]*lib\/router\/routes"/.test(out)) {
    const rel = relative(dirname(file), join(sites, "packages", "core", "src", "lib", "router", "routes")).replace(/\\/g, "/");
    const importLine = `import { href } from "${rel.startsWith(".") ? rel : "./" + rel}";`;
    const anchor = out.match(/^import [^\n]* from "react-router";$/m);
    if (anchor) out = out.replace(anchor[0], `${anchor[0]}\n${importLine}`);
    else out = out.replace(/^(import [^\n]*\n)/, `$1${importLine}\n`);
  }
  writeFileSync(file, out);
  filesChanged += 1;
}

console.log(`rewrote ${rewrites} links in ${filesChanged} files`);
for (const [file, what] of stragglers) console.log(`straggler: ${relative(sites, file)}: ${what}`);
