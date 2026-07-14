#!/usr/bin/env node
// foundry-gen-skills-catalog — snapshots the real skill libraries the copilot
// mounts into a committed JSON fixture.
//
//   npm run foundry:gen-skills-catalog -- [--sdk-skills <dir>] [--workspace <dir>] [--check]
//
// Two libraries, both real, neither re-authored here: decentraland/sdk-skills
// (mounted via `skills.paths`) and the pre-production GDD pair that ships in the
// copilot workspace. Name and description are the frontmatter the model itself
// selects on; the dates are `git log --follow` on the skill's directory, so a
// skill that has not been touched since April says April.
//
// The result is committed because the built server must not depend on a mirror
// checkout being present on the machine that serves the page.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SITES = fileURLToPath(new URL("..", import.meta.url));
const OUT = join(SITES, "packages/data/src/fixtures/sdk-skills-catalog.json");

const DEFAULT_SDK_SKILLS = process.env.SDK_SKILLS_DIR ?? join(SITES, "../../github.com-decentraland/sdk-skills");
const DEFAULT_WORKSPACE = join(SITES, "../deploy/copilot/workspace");

function flag(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i === -1 ? undefined : process.argv[i + 1];
}

type Source = "sdk-skills" | "pre-prod" | "command";

interface Entry {
  name: string;
  description: string;
  dir: string;
  firstCommit: string | null;
  lastCommit: string | null;
  source: Source;
}

function gitDates(repo: string, path: string): { first: string | null; last: string | null } {
  try {
    const out = execFileSync(
      "git",
      ["-C", repo, "log", "--follow", "--format=%aI", "--", path],
      { encoding: "utf8", timeout: 30_000 },
    )
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);
    if (out.length === 0) return { first: null, last: null };
    return { first: out[out.length - 1], last: out[0] };
  } catch {
    // Not a checkout, or git is unavailable: the entry ships without dates
    // rather than with invented ones.
    return { first: null, last: null };
  }
}

/**
 * Pulls `name` and `description` out of a SKILL.md header.
 *
 * Deliberately not a YAML parse: these skills are vendored verbatim from their
 * authors and several carry unquoted descriptions full of colons, which a strict
 * loader rejects. The agent runtimes read them line-wise, and so do we — the
 * alternative is editing someone else's file to suit our tooling.
 */
function skillHeader(md: string): { name?: string; description?: string } {
  const block = /^---\r?\n([\s\S]*?)\r?\n---/.exec(md)?.[1];
  if (!block) return {};
  const field = (key: string): string | undefined => {
    const line = new RegExp(`^${key}:[ \\t]*(.*)$`, "m").exec(block)?.[1]?.trim();
    if (!line) return undefined;
    return line.replace(/^"(.*)"$/s, "$1").replace(/^'(.*)'$/s, "$1").trim();
  };
  return { name: field("name"), description: field("description") };
}

/** Every `<dir>/*&#47;SKILL.md`, in name order. */
function readLibrary(
  root: string,
  source: Source,
  dirLabel: (name: string) => string,
  repoForDates: string | null,
): Entry[] {
  if (!existsSync(root)) return [];
  const names = readdirSync(root, { withFileTypes: true })
    .filter((e) => e.isDirectory() && existsSync(join(root, e.name, "SKILL.md")))
    .map((e) => e.name)
    .sort();

  return names.map((name) => {
    const fm = skillHeader(readFileSync(join(root, name, "SKILL.md"), "utf8"));
    const dates = repoForDates
      ? gitDates(repoForDates, relative(repoForDates, join(root, name)) || name)
      : { first: null, last: null };
    return {
      name: fm.name?.trim() || name,
      description: (fm.description ?? "").trim(),
      dir: dirLabel(name),
      firstCommit: dates.first,
      lastCommit: dates.last,
      source,
    };
  });
}

// Slash commands are flat .md files beside the skills — the copilot's typed
// doorway (/gdd, /brief). The roster must list them: the gdd page's publish
// instructions name them, and a doorway the roster hides is a dead end.
function readCommands(root: string): Entry[] {
  if (!existsSync(root)) return [];
  const names = readdirSync(root, { withFileTypes: true })
    .filter((e) => e.isFile() && e.name.endsWith(".md"))
    .map((e) => e.name.replace(/\.md$/, ""))
    .sort();
  return names.map((name) => {
    const fm = skillHeader(readFileSync(join(root, `${name}.md`), "utf8"));
    return {
      name: `/${name}`,
      description: (fm.description ?? "").trim(),
      dir: `copilot workspace .opencode/commands/${name}.md`,
      firstCommit: null,
      lastCommit: null,
      source: "command" as const,
    };
  });
}

const sdkRoot = resolve(flag("sdk-skills") ?? DEFAULT_SDK_SKILLS);
const workspace = resolve(flag("workspace") ?? DEFAULT_WORKSPACE);

const entries: Entry[] = [
  ...readLibrary(sdkRoot, "sdk-skills", (n) => `sdk-skills/${n}`, sdkRoot),
  ...readLibrary(
    join(workspace, ".opencode/skills"),
    "pre-prod",
    (n) => `copilot workspace .opencode/skills/${n}`,
    null,
  ),
  ...readCommands(join(workspace, ".opencode/commands")),
];

if (entries.length === 0) {
  console.error(
    `foundry-gen-skills-catalog: no SKILL.md found under ${sdkRoot} or ${workspace}`,
  );
  process.exit(1);
}

// Provenance paths ship in the committed fixture, so keep them machine-neutral:
// a home-anchored absolute path is a private-layout leak the export gate
// rejects.
const portable = (p: string) => p.replace(homedir(), "~");

const catalog = {
  generatedFrom: {
    sdkSkills: portable(sdkRoot),
    workspace: portable(workspace),
    readAt: new Date().toISOString().slice(0, 10),
    note:
      "name and description are each skill's own frontmatter; dates are git log --follow on the skill directory. Skills outside a git checkout ship without dates rather than with guessed ones.",
  },
  skills: entries,
};

if (process.argv.includes("--check")) {
  const current = existsSync(OUT) ? JSON.parse(readFileSync(OUT, "utf8")) : null;
  const same =
    current && JSON.stringify(current.skills) === JSON.stringify(catalog.skills);
  if (!same) {
    console.error("foundry-gen-skills-catalog: catalog is stale — re-run without --check");
    process.exit(1);
  }
  console.log(`foundry-gen-skills-catalog: ${entries.length} skills, catalog up to date`);
} else {
  writeFileSync(OUT, `${JSON.stringify(catalog, null, 2)}\n`);
  const bySource = entries.reduce<Record<string, number>>((acc, e) => {
    acc[e.source] = (acc[e.source] ?? 0) + 1;
    return acc;
  }, {});
  console.log(
    `foundry-gen-skills-catalog: wrote ${entries.length} skills ` +
      `(${Object.entries(bySource)
        .map(([k, v]) => `${v} ${k}`)
        .join(", ")}) to ${relative(SITES, OUT)}`,
  );
}
