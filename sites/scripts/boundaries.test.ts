import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SITES = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PACKAGES = path.join(SITES, "packages");

type Boundary = { alias: string; root: string; mayImport: string[] };
type Pkg = { name: string; dir: string; absRoot: string; boundary: Boundary };

const packages: Pkg[] = fs
  .readdirSync(PACKAGES, { withFileTypes: true })
  .filter((e) => e.isDirectory() && !e.name.startsWith("."))
  .map((e) => {
    const dir = path.join(PACKAGES, e.name);
    const manifest = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
    return { name: manifest.name, dir, absRoot: path.join(dir, manifest.sitesBoundary.root), boundary: manifest.sitesBoundary };
  });

const byAlias = new Map(packages.map((p) => [p.boundary.alias, p]));
const nameOf = new Map(packages.map((p) => [p.boundary.alias, p.name]));

const SOURCE_EXT = new Set([".ts", ".tsx", ".mts", ".mjs", ".js", ".jsx"]);

function walk(dir: string, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (SOURCE_EXT.has(path.extname(e.name))) out.push(p);
  }
  return out;
}

const SPEC_RE =
  /(?:^|[\s;}(])(?:import|export)\s+(?:type\s+)?(?:[\w*{},\s]*?\s+from\s+)?["']([^"']+)["']|import\s*\(\s*["']([^"']+)["']\s*\)/g;

function specifiers(src: string): string[] {
  const out: string[] = [];
  SPEC_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = SPEC_RE.exec(src))) {
    const s = m[1] ?? m[2];
    if (s) out.push(s);
  }
  return out;
}

const owner = (abs: string) => packages.find((p) => abs.startsWith(p.absRoot + path.sep));
const rel = (abs: string) => path.relative(SITES, abs);

// Upward imports that predate the package split. `data` reaches up into `features`
// for the port-contract types its XState machines own (FulfillFn, CommitFn,
// SummaryView, PublishCollection) and two tests reach up into `routes` for the
// loader/action they exercise. Each entry is debt: move the contract type down
// into `data`, or move the test into the package that owns its subject. Adding a
// line here is not a fix -- the test above fails for anything not listed.
const WAIVED = new Set([
  "packages/data/src/lib/auth/pair-store.test.ts -> @routes/routes/internal.pair",
  "packages/data/src/lib/catalyst/creator-hub/scene-drafts.test.ts -> @routes/routes/api.creator-hub.drafts.$",
  "packages/data/src/lib/catalyst/creator-hub/wearable-publish-collection.server.ts -> @features/stories/creator-hub/wearable-publish-collection/machine",
  "packages/data/src/lib/catalyst/creator-hub/wearable-publish-collection.server.ts -> @features/stories/creator-hub/wearable-publish-collection/PublishCollectionWizard",
  "packages/data/src/lib/catalyst/creator-hub/wearable-publish-collection.ts -> @features/stories/creator-hub/wearable-publish-collection/machine",
  "packages/data/src/lib/catalyst/creator-hub/wearable-publish-collection.ts -> @features/stories/creator-hub/wearable-publish-collection/PublishCollectionWizard",
  "packages/data/src/lib/catalyst/landings/rsvp.ts -> @features/stories/landings/rsvp-event/machine",
  "packages/data/src/lib/catalyst/landings/subscriptions.ts -> @features/stories/landings/event-subscriptions/machine",
  "packages/data/src/lib/catalyst/marketplace/buy-min-clicks.test.ts -> @features/stories/marketplace/checkout/machine",
  "packages/data/src/lib/catalyst/marketplace/checkout-run.ts -> @features/stories/marketplace/checkout/machine",
]);

function importViolations(): { key: string; message: string }[] {
  const out: { key: string; message: string }[] = [];
  for (const p of packages) {
    const allowed = new Set(p.boundary.mayImport);
    for (const file of walk(p.absRoot)) {
      for (const spec of specifiers(fs.readFileSync(file, "utf8"))) {
        const aliasHit = [...byAlias.keys()].find((a) => spec === a || spec.startsWith(a + "/"));
        if (!aliasHit) continue;
        const target = nameOf.get(aliasHit)!;
        if (target === p.name) {
          out.push({
            key: `${rel(file)} -> SELF ${spec}`,
            message: `${rel(file)}: uses ${aliasHit} to import its own package (use a relative path)`,
          });
          continue;
        }
        if (!allowed.has(target)) {
          out.push({
            key: `${rel(file)} -> ${spec}`,
            message: `${rel(file)}: ${p.name} may not import ${target} \u{2014} "${spec}"`,
          });
        }
      }
    }
  }
  return out;
}

describe("package boundaries", () => {
  it("every package declares a boundary and they form the intended layering", () => {
    expect(packages.map((p) => p.name).sort()).toEqual([
      "@sites/core",
      "@sites/data",
      "@sites/features",
      "@sites/routes",
    ]);
    for (const p of packages) {
      expect(fs.existsSync(p.absRoot), `${p.name} root ${p.boundary.root} exists`).toBe(true);
      for (const dep of p.boundary.mayImport) {
        expect(packages.some((q) => q.name === dep), `${p.name} mayImport ${dep} names a real package`).toBe(true);
      }
    }
  });

  it("no package imports a package it may not depend on", () => {
    const violations = importViolations();
    expect(violations.filter((v) => !WAIVED.has(v.key)).map((v) => v.message)).toEqual([]);
  });

  it("every waived upward import still exists (the waiver list only shrinks)", () => {
    const live = new Set(importViolations().map((v) => v.key));
    const stale = [...WAIVED].filter((k) => !live.has(k));
    expect(stale, "delete these from WAIVED \u{2014} the import they excused is gone").toEqual([]);
  });

  it("no relative import escapes its own package", () => {
    const violations: string[] = [];
    for (const p of packages) {
      for (const file of walk(p.absRoot)) {
        for (const spec of specifiers(fs.readFileSync(file, "utf8"))) {
          if (!spec.startsWith(".")) continue;
          const abs = path.resolve(path.dirname(file), spec);
          if (abs.startsWith(p.absRoot + path.sep)) continue;
          if (!abs.startsWith(PACKAGES + path.sep)) continue;
          const target = owner(abs);
          violations.push(
            `${rel(file)}: relative import "${spec}" escapes ${p.name} into ${target?.name ?? "another package"} \u{2014} use its alias`,
          );
        }
      }
    }
    expect(violations).toEqual([]);
  });

  // Filesystem reads are a back-channel the import graph cannot see: a module can
  // reach into another package by building a path string. The set below is pinned
  // so a new one fails this test; shrinking it is always safe.
  it("cross-package filesystem coupling stays within the pinned inventory", () => {
    const roots = packages.map((p) => {
      const segs = path.relative(SITES, p.absRoot).split(path.sep);
      return {
        pkg: p,
        // path.join("packages", "data", "src", ...) in any formatting
        joined: segs.map((s) => JSON.stringify(s)).join(","),
        // "packages/data/src/..." as one literal
        slash: segs.join("/"),
      };
    });
    const found: string[] = [];
    for (const p of packages) {
      const allowed = new Set(p.boundary.mayImport);
      for (const file of walk(p.absRoot)) {
        const src = fs.readFileSync(file, "utf8");
        const dense = src.replace(/\s+/g, "");
        for (const r of roots) {
          if (r.pkg.name === p.name || allowed.has(r.pkg.name)) continue;
          if (dense.includes(r.joined) || src.includes(r.slash)) {
            found.push(`${rel(file)} -> ${r.pkg.name}`);
          }
        }
      }
    }
    expect(found.sort()).toEqual([
      "packages/core/src/lib/experiments/create-entry.test.ts -> @sites/features",
      "packages/core/src/lib/experiments/open-screen.test.ts -> @sites/features",
      "packages/core/src/lib/experiments/story-loader.ts -> @sites/features",
      "packages/core/src/lib/landings/creator-hub-download.server.ts -> @sites/data",
      "packages/data/src/lib/catalyst/places/presence.test.ts -> @sites/features",
    ]);
  });

  it("only the routes package holds SSR entry points and route modules", () => {
    const routes = packages.find((p) => p.name === "@sites/routes")!;
    for (const entry of ["root.tsx", "entry.client.tsx", "entry.server.tsx", "routes.ts"]) {
      expect(fs.existsSync(path.join(routes.absRoot, entry)), `routes package owns ${entry}`).toBe(true);
    }
    for (const p of packages) {
      if (p.name === routes.name) continue;
      for (const file of walk(p.absRoot)) {
        expect(
          /\/\+types\//.test(fs.readFileSync(file, "utf8")),
          `${rel(file)} uses react-router route typegen outside the routes package`,
        ).toBe(false);
      }
    }
  });
});
