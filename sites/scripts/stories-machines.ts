import { readdirSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { toDirectedGraph } from "@xstate/graph";

const HERE = dirname(fileURLToPath(import.meta.url));
const STORIES = join(HERE, "..", "packages", "features", "src", "stories");

const RESERVED = new Set([
  "state",
  "note",
  "as",
  "hide",
  "direction",
  "end",
  "class",
  "click",
  "call",
  "link",
]);

function findMachines(dir: string): string[] {
  const out: string[] = [];
  (function walk(d: string) {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name === "machine.ts") out.push(p);
    }
  })(dir);
  return out.sort();
}

function isMachine(v: any): boolean {
  return v && typeof v === "object" && v.root && typeof v.transition === "function";
}

function prettyEvent(ev: string | undefined | null): string {
  if (!ev) return "(always)";
  let m: RegExpMatchArray | null;
  if ((m = ev.match(/^xstate\.done\.actor\.(.+)$/))) return "\u{2713} " + m[1];
  if ((m = ev.match(/^xstate\.error\.actor\.(.+)$/))) return "\u{2717} " + m[1];
  if ((m = ev.match(/^xstate\.done\.state\.(.+)$/)))
    return "\u{2713} " + m[1].split(".").pop();
  if ((m = ev.match(/^xstate\.after\.(\d+)\.(.+)$/))) return "after " + m[1] + "ms";
  if (ev === "*") return "*";
  return ev;
}

function escapeLabel(s: string): string {
  return s.replace(/:/g, "\u{B7}").replace(/\n/g, " ").replace(/[{}]/g, "");
}

// Both outputs are committed; serialization must be pure ASCII so regeneration
// is byte-stable (md renders glyphs as words, html as numeric entities).
function mdAscii(s: string): string {
  return s
    .replace(/\u{2713}/gu, "OK")
    .replace(/\u{2717}/gu, "x")
    .replace(/\u{2192}/gu, "->")
    .replace(/\u{B7}/gu, "-");
}

function htmlAscii(s: string): string {
  return s.replace(
    /[^\x00-\x7F]/gu,
    (c) => `&#x${c.codePointAt(0)!.toString(16).toUpperCase()};`,
  );
}

type Built = { mermaid: string; states: number; exportName: string };

function buildMermaid(mod: any): Built {
  const entry = Object.entries(mod).find(([, v]) => isMachine(v)) ??
    (isMachine(mod.default) ? ["default", mod.default] : undefined);
  if (!entry) throw new Error("no machine export");
  const exportName = entry[0];
  const machine: any = entry[1];
  const g: any = toDirectedGraph(machine);
  const rootId: string = g.id;

  const short = (fullId: string) =>
    fullId.startsWith(rootId + ".") ? fullId.slice(rootId.length + 1) : "";

  const keyCount: Record<string, number> = {};
  let stateCount = 0;
  (function count(n: any) {
    for (const c of n.children) {
      stateCount++;
      keyCount[c.stateNode.key] = (keyCount[c.stateNode.key] || 0) + 1;
      count(c);
    }
  })(g);
  const unique = Object.values(keyCount).every((n) => n === 1);

  const idCache = new Map<string, string>();
  function idOf(fullId: string): string {
    if (idCache.has(fullId)) return idCache.get(fullId)!;
    const s = short(fullId);
    const parts = s.split(".");
    let id = (unique ? parts[parts.length - 1] : parts.join("_")).replace(
      /[^A-Za-z0-9_]/g,
      "_",
    );
    if (RESERVED.has(id) || /^\d/.test(id)) id = "s_" + id;
    idCache.set(fullId, id);
    return id;
  }

  const initialFull = (node: any): string | null => {
    const t = node.stateNode?.initial?.target;
    if (Array.isArray(t) && t.length) {
      let f = t[0];
      if (f && f.id) f = f.id;
      return typeof f === "string" ? f.replace(/^#/, "") : null;
    }
    return null;
  };

  const lines: string[] = ["stateDiagram-v2"];
  const ri = initialFull(g);
  if (ri) lines.push(`  [*] --> ${idOf(ri)}`);

  function structure(node: any, indent: string) {
    for (const c of node.children) {
      const cid = idOf(c.id);
      const key = c.stateNode.key;
      if (c.children.length) {
        lines.push(`${indent}state ${cid} {`);
        const it = initialFull(c);
        if (it) lines.push(`${indent}  [*] --> ${idOf(it)}`);
        structure(c, indent + "  ");
        lines.push(`${indent}}`);
      } else if (cid !== key) {
        lines.push(`${indent}state "${key}" as ${cid}`);
      } else {
        lines.push(`${indent}${cid}`);
      }
    }
  }
  structure(g, "  ");

  const globals: string[] = [];
  function edges(node: any) {
    const isRoot = node.id === rootId;
    for (const e of node.edges) {
      const label = (() => {
        let l = prettyEvent(e.transition?.eventType);
        const guard = e.transition?.guard;
        const gName = typeof guard === "string" ? guard : guard && guard.type;
        if (gName) l += ` [${gName}]`;
        return escapeLabel(l);
      })();
      const tid = e.target?.id ? idOf(e.target.id) : "";
      if (isRoot) {
        globals.push(tid ? `${label} \u{2192} ${tid}` : `${label} (internal)`);
      } else {
        const src = idOf(node.id);
        lines.push(`  ${src} --> ${tid || src} : ${label}`);
      }
    }
    for (const c of node.children) edges(c);
  }
  edges(g);

  (function finals(node: any) {
    for (const c of node.children) {
      if (c.stateNode.type === "final") lines.push(`  ${idOf(c.id)} --> [*]`);
      finals(c);
    }
  })(g);

  if (globals.length) {
    lines.push(`  note right of ${idOf(ri ?? rootId)}`);
    lines.push(`    global (from any state):`);
    for (const gtxt of globals) lines.push(`    ${gtxt}`);
    lines.push(`  end note`);
  }

  return { mermaid: lines.join("\n"), states: stateCount, exportName };
}

const files = findMachines(STORIES);
type Entry = { surface: string; name: string; built: Built };
const entries: Entry[] = [];
const failures: string[] = [];

for (const f of files) {
  const rel = f.slice(STORIES.length + 1).replace(/\/machine\.ts$/, "");
  const surface = rel.split("/")[0];
  try {
    const mod = await import(pathToFileURL(f).href);
    entries.push({ surface, name: rel, built: buildMermaid(mod) });
  } catch (e: any) {
    failures.push(`${rel}: ${String(e.message || e).split("\n")[0]}`);
  }
}

const bySurface = new Map<string, Entry[]>();
for (const e of entries) {
  if (!bySurface.has(e.surface)) bySurface.set(e.surface, []);
  bySurface.get(e.surface)!.push(e);
}
const surfaces = [...bySurface.keys()].sort();

const md: string[] = [];
md.push("# Story state machines");
md.push("");
md.push(
  `_Auto-generated by \`scripts/stories-machines.ts\` (\`npm run story:machines\`). ${entries.length} machines across ${surfaces.length} surfaces. Diagrams render inline on GitHub / VS Code._`,
);
md.push("");
md.push("| Surface | Machines |");
md.push("| --- | --- |");
for (const s of surfaces) md.push(`| [${s}](#${s}) | ${bySurface.get(s)!.length} |`);
md.push("");
for (const s of surfaces) {
  md.push(`## ${s}`);
  md.push("");
  for (const e of bySurface.get(s)!) {
    md.push(`### ${e.name}`);
    md.push(
      `\`${e.built.exportName}\` - ${e.built.states} states - [source](./${e.name}/machine.ts)`,
    );
    md.push("");
    md.push("```mermaid");
    md.push(e.built.mermaid);
    md.push("```");
    md.push("");
  }
}
if (failures.length) {
  md.push("## skipped");
  md.push("");
  for (const f of failures) md.push(`- ${f}`);
  md.push("");
}
writeFileSync(join(STORIES, "MACHINES.md"), mdAscii(md.join("\n")));

const cards = entries
  .map(
    (e) => `<article class="card" data-name="${e.name}" data-surface="${e.surface}">
  <h3>${e.name} <small>${e.built.exportName} \u{B7} ${e.built.states} states</small></h3>
  <pre class="mermaid">${e.built.mermaid
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")}</pre>
</article>`,
  )
  .join("\n");

const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>Story state machines</title>
<style>
  :root { color-scheme: light dark; }
  body { font: 15px/1.5 system-ui, sans-serif; margin: 0; padding: 1.25rem; max-width: 1100px; margin-inline: auto; }
  h1 { margin: 0 0 .25rem; }
  .meta { color: #888; margin-bottom: 1rem; }
  #q { width: 100%; padding: .6rem .8rem; font-size: 1rem; border: 1px solid #8884; border-radius: .5rem; margin-bottom: 1rem; box-sizing: border-box; }
  .card { border: 1px solid #8883; border-radius: .6rem; padding: .75rem 1rem; margin: .75rem 0; }
  .card h3 { margin: .1rem 0 .5rem; font-size: 1.05rem; }
  .card small { color: #888; font-weight: 400; font-size: .8rem; }
  .surface { font-size: .8rem; text-transform: uppercase; letter-spacing: .05em; color: #888; margin: 1.5rem 0 .25rem; }
  .mermaid { overflow-x: auto; }
  .hidden { display: none; }
</style></head>
<body>
  <h1>Story state machines</h1>
  <div class="meta">${entries.length} machines \u{B7} ${surfaces.length} surfaces \u{B7} generated by scripts/stories-machines.ts</div>
  <input id="q" type="search" placeholder="filter by name\u{2026} (e.g. marketplace, vote, deploy)" autofocus />
  <div id="list">
${cards}
  </div>
  <script type="module">
    import mermaid from "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs";
    mermaid.initialize({ startOnLoad: false, theme: "default", securityLevel: "loose" });
    await mermaid.run({ querySelector: ".mermaid" });
    const q = document.getElementById("q");
    const cards = [...document.querySelectorAll(".card")];
    q.addEventListener("input", () => {
      const t = q.value.trim().toLowerCase();
      for (const c of cards)
        c.classList.toggle("hidden", t && !c.dataset.name.toLowerCase().includes(t) && !c.dataset.surface.toLowerCase().includes(t));
    });
  </script>
</body></html>`;
writeFileSync(join(STORIES, "machines.html"), htmlAscii(html));

console.log(
  `wrote packages/features/src/stories/MACHINES.md + machines.html - ${entries.length} machines, ${surfaces.length} surfaces${failures.length ? `, ${failures.length} skipped` : ""}`,
);
