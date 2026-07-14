import { execSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { SECTIONS as CREATORHUB_SECTIONS } from "@ui/creatorhub/pages/chflowmapdata";
import { SECTIONS as EXPLORER_SECTIONS } from "@ui/explorer/pages/exflowmapdata";
import type { FlowSection, Track } from "@ui/flowmap/flowmapdata";

const MANIFEST_FILE = ".react-router/types/+routes.ts";

const EXTERNAL_MOUNTS = new Set(["/play/"]);

function routeManifest(): Set<string> {
  if (!existsSync(MANIFEST_FILE)) execSync("npx react-router typegen", { stdio: "inherit" });
  const src = readFileSync(MANIFEST_FILE, "utf8");
  const block = src.match(/type Pages = \{([\s\S]*?)\n\};/);
  if (!block) throw new Error(`Pages block not found in ${MANIFEST_FILE}`);
  return new Set([...block[1].matchAll(/^ {2}"([^"]+)": \{/gm)].map((m) => m[1]));
}

function collectRoutes(datasets: [string, FlowSection[]][]): Map<string, string[]> {
  const found = new Map<string, string[]>();
  for (const [name, sections] of datasets) {
    const walk = (track: Track, section: string) => {
      for (const item of track.items) {
        if (item.t === "node" && item.kind === "route") {
          const at = `${name}#${section}`;
          const prev = found.get(item.label) ?? [];
          found.set(item.label, [...prev, at]);
        }
      }
      track.branches?.forEach((b) => walk(b, section));
    };
    for (const s of sections) s.tracks.forEach((t) => walk(t, s.id));
  }
  return found;
}

const manifest = routeManifest();
const routes = collectRoutes([
  ["chflowmapdata", CREATORHUB_SECTIONS],
  ["exflowmapdata", EXPLORER_SECTIONS],
]);

const dead: string[] = [];
for (const [label, where] of routes) {
  const path = label.replace(/[?#].*$/, "");
  if (manifest.has(path) || EXTERNAL_MOUNTS.has(path)) continue;
  dead.push(`${label} (${[...new Set(where)].join(", ")})`);
}

if (dead.length) {
  console.error(`flowmap route gate: ${dead.length} route node(s) not in the route manifest:`);
  for (const d of dead) console.error(`  ${d}`);
  process.exit(1);
}
console.log(`flowmap route gate: ${routes.size} route nodes all present in the route manifest`);
