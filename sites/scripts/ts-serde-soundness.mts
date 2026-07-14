import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const PARSEABLE = new Set(["rename", "skip", "flatten", "default", "with"]);
const OMITTING = new Set(["skip_serializing_if", "skip_serializing"]);
const ITEM_RE = /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+\w+/;

function rustFiles(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    if (entry === "target") continue;
    const p = join(dir, entry);
    const st = statSync(p);
    if (st.isDirectory()) rustFiles(p, out);
    else if (entry.endsWith(".rs")) out.push(p);
  }
  return out;
}

function collectAttr(lines: string[], start: number): { text: string; end: number } {
  let depth = 0;
  let end = start;
  const parts: string[] = [];
  for (let j = start; j < lines.length; j++) {
    const t = lines[j]!.trim();
    parts.push(t);
    depth += (t.match(/\[/g) ?? []).length - (t.match(/\]/g) ?? []).length;
    end = j;
    if (depth <= 0) break;
  }
  return { text: parts.join(" "), end };
}

function splitTopLevel(inner: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let cur = "";
  let inStr = false;
  for (const ch of inner) {
    if (ch === '"') inStr = !inStr;
    if (!inStr) {
      if (ch === "(" || ch === "[") depth += 1;
      else if (ch === ")" || ch === "]") depth -= 1;
      if (ch === "," && depth === 0) {
        parts.push(cur.trim());
        cur = "";
        continue;
      }
    }
    cur += ch;
  }
  if (cur.trim()) parts.push(cur.trim());
  return parts;
}

function serdeKeys(attrText: string): string[] | null {
  const m = attrText.match(/^#\[serde\((.*)\)\]$/);
  if (!m) return null;
  return splitTopLevel(m[1]!).map((p) => p.split("=")[0]!.trim());
}

function checkFile(path: string, violations: string[]): void {
  const lines = readFileSync(path, "utf8").split("\n");
  let i = 0;
  while (i < lines.length) {
    if (!ITEM_RE.test(lines[i]!) || lines[i]!.trim().startsWith("//")) {
      i += 1;
      continue;
    }
    let k = i - 1;
    const headAttrs: string[] = [];
    while (k >= 0) {
      const t = lines[k]!.trim();
      if (t === "" || t.startsWith("///") || t.startsWith("//")) {
        k -= 1;
        continue;
      }
      let s = k;
      while (s >= 0 && !lines[s]!.trim().startsWith("#[")) s -= 1;
      if (s < 0) break;
      const { end } = collectAttr(lines, s);
      if (end !== k) break;
      headAttrs.push(collectAttr(lines, s).text);
      k = s - 1;
    }
    const derivesTs = headAttrs.some((a) => /\bderive\b[^\]]*\bTS\b/.test(a));
    if (!derivesTs || !lines[i]!.includes("{")) {
      i += 1;
      continue;
    }
    let depth = 0;
    let j = i;
    do {
      depth +=
        (lines[j]!.match(/\{/g) ?? []).length -
        (lines[j]!.match(/\}/g) ?? []).length;
      j += 1;
    } while (j < lines.length && depth > 0);
    const bodyEnd = j;
    let f = i + 1;
    while (f < bodyEnd) {
      const t = lines[f]!.trim();
      if (!t.startsWith("#[")) {
        f += 1;
        continue;
      }
      const group: { text: string; line: number }[] = [];
      let gl = f;
      while (gl < bodyEnd) {
        const gt = lines[gl]!.trim();
        if (gt.startsWith("#[")) {
          const { text, end } = collectAttr(lines, gl);
          group.push({ text, line: gl + 1 });
          gl = end + 1;
        } else if (gt.startsWith("///") || gt === "") {
          gl += 1;
        } else {
          break;
        }
      }
      const groupText = group.map((g) => g.text).join(" ");
      const hasTsOptional = /\bts\((?:[^()"]|"[^"]*"|\([^()]*\))*\boptional\b/.test(
        groupText,
      );
      const hasTsShape = /\bts\((?:[^()"]|"[^"]*"|\([^()]*\))*\b(?:type|as)\s*=/.test(
        groupText,
      );
      for (const g of group) {
        const keys = serdeKeys(g.text);
        if (!keys) continue;
        const bad = keys.filter((x) => !PARSEABLE.has(x));
        const omitting = keys.filter((x) => OMITTING.has(x));
        const good = keys.filter((x) => PARSEABLE.has(x));
        if (omitting.length > 0 && !hasTsOptional) {
          violations.push(
            `${path}:${g.line}: ${omitting[0]} on a ts-rs field without #[ts(optional)] \u{2014} generated TS declares the field present while serde omits it`,
          );
        }
        if (bad.length > 0 && good.length > 0) {
          violations.push(
            `${path}:${g.line}: serde attr mixes ${good.join("/")} with ${bad.join("/")} \u{2014} ts-rs 10 drops the whole attr, silently ignoring ${good.join("/")}; split into separate #[serde(...)] attrs`,
          );
        }
        if (keys.includes("serialize_with") && !hasTsShape) {
          violations.push(
            `${path}:${g.line}: serialize_with on a ts-rs field without #[ts(type/as)] \u{2014} the custom serializer's wire shape is not reflected in the generated TS`,
          );
        }
      }
      f = gl + 1;
    }
    i = bodyEnd;
  }
}

const crateDirs = process.argv.slice(2);
if (crateDirs.length === 0) {
  console.error("usage: node ts-serde-soundness.mts <crate-dir>...");
  process.exit(2);
}
const violations: string[] = [];
for (const dir of crateDirs) {
  for (const file of rustFiles(join(dir, "src"))) checkFile(file, violations);
}
if (violations.length > 0) {
  console.error("ts-rs serde soundness check FAILED:");
  for (const v of violations) console.error(`  ${v}`);
  process.exit(1);
}
console.log(
  `ts-rs serde soundness: OK (${crateDirs.length} crates, no unsound serde attrs)`,
);
