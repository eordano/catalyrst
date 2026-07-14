#!/usr/bin/env node
import { readdirSync, readFileSync, statSync, writeFileSync, existsSync } from "node:fs";
import { join, relative, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../../..");
const SRC = join(repo, "bevy-explorer/editor-scene/node_modules/@dcl");
const OUT = join(repo, "catalyrst/ui3/public/dcl-sdk-types.json");

const PACKAGES = ["sdk", "ecs", "ecs-math", "js-runtime", "react-ecs"];
const SKIP_DIRS = new Set(["node_modules", ".bin", "dist-cjs", "coverage"]);

function harvest(pkg: string): Record<string, string> {
  const root = join(SRC, pkg);
  if (!existsSync(root)) {
    console.error(`missing package: @dcl/${pkg} under ${SRC}`);
    process.exit(1);
  }
  const files: Record<string, string> = {};
  const walk = (dir: string): void => {
    for (const name of readdirSync(dir)) {
      if (SKIP_DIRS.has(name)) continue;
      const p = join(dir, name);
      const st = statSync(p);
      if (st.isDirectory()) {
        walk(p);
      } else if (name.endsWith(".d.ts") || name === "package.json") {
        const rel = relative(root, p).replaceAll("\\", "/");
        files[`@dcl/${pkg}/${rel}`] = readFileSync(p, "utf8");
      }
    }
  };
  walk(root);
  return files;
}

const bundle: Record<string, string> = {};
for (const pkg of PACKAGES) Object.assign(bundle, harvest(pkg));

const meta = {
  generated: "catalyrst/sites/scripts/gen-dcl-types.mts",
  source: "bevy-explorer/editor-scene/node_modules/@dcl",
  packages: PACKAGES,
  fileCount: Object.keys(bundle).length,
};
writeFileSync(OUT, JSON.stringify({ meta, files: bundle }));
const bytes = statSync(OUT).size;
console.log(
  `wrote ${OUT}: ${meta.fileCount} files, ${(bytes / 1024 / 1024).toFixed(2)} MB raw`,
);
