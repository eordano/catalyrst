import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = join(__dirname, "..");
const SCOPES = ["admin", "operator"];
const LEGACY = /^web\/pages\/stwhatsonadmin/;
const SHARED = "admin/admin.css";
const COLOR = /#[0-9a-f]{3,8}\b|\b(?:rgba?|hsla?)\(|\b(?:white|black|red|blue|green|gray|grey|orange|yellow)\b(?!-)/i;
const TOKEN_LINE = /^\s*--adm-[a-z0-9-]+\s*:/;
const ALLOWED_CLASS = /^(adm(?:$|-|__)|is-|has-|btn|u-|search|modal|es(?:$|-|__)|filter-radios|spinner)/;

function cssFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) out.push(...cssFiles(p));
    else if (name.endsWith(".css")) out.push(p);
  }
  return out;
}

const files = [
  ...SCOPES.flatMap((s) => cssFiles(join(ROOT, s))),
  ...cssFiles(join(ROOT, "web", "pages")).filter((p) => LEGACY.test(relative(ROOT, p))),
].map((p) => ({ rel: relative(ROOT, p), text: readFileSync(p, "utf8").replace(/\/\*[\s\S]*?\*\//g, "") }));

function selectors(text: string): string[] {
  const out: string[] = [];
  let buf = "";
  for (const ch of text) {
    if (ch === "{") {
      const sel = buf.trim().replace(/\s+/g, " ");
      if (sel && !sel.startsWith("@")) out.push(sel);
      buf = "";
    } else if (ch === "}" || ch === ";") {
      buf = "";
    } else {
      buf += ch;
    }
  }
  return out;
}

function classNames(selector: string): string[] {
  return [...selector.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)].map((m) => m[1] ?? "");
}

describe("admin console stylesheets", () => {
  it("keep every color in a token", () => {
    expect(files.some((f) => f.rel === SHARED)).toBe(true);
    const offenders = files.flatMap((f) =>
      f.text
        .split("\n")
        .map((line, i) => ({ line, n: i + 1 }))
        .filter(({ line }) => !TOKEN_LINE.test(line) && COLOR.test(line))
        .map(({ line, n }) => `${f.rel}:${n}: ${line.trim()}`),
    );
    expect(offenders).toEqual([]);
  });

  it("only define adm classes", () => {
    const offenders = files.flatMap((f) =>
      selectors(f.text)
        .flatMap((sel) => classNames(sel).map((c) => ({ sel, c })))
        .filter(({ c }) => !ALLOWED_CLASS.test(c))
        .map(({ sel, c }) => `${f.rel}: ${c} in "${sel}"`),
    );
    expect([...new Set(offenders)]).toEqual([]);
  });

  it("never redefine a shared rule in a page stylesheet", () => {
    const shared = files.find((f) => f.rel === SHARED)!;
    const sharedSelectors = new Set(selectors(shared.text).filter((s) => !s.startsWith("@")));
    const offenders = files
      .filter((f) => f.rel !== SHARED)
      .flatMap((f) => selectors(f.text).filter((s) => sharedSelectors.has(s)).map((s) => `${f.rel}: ${s}`));
    expect(offenders).toEqual([]);
  });
});
