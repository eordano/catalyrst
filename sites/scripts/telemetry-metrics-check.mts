import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import matter from "gray-matter";

// Gate: every event a story.md's metric binds must resolve to a registered
// telemetry event. Without this, a metric that names a non-emitted event (a
// verb-stem label vs a past-tense event, a typo, an unwired draft event) reads
// a silent 0 at readout -- indistinguishable from "no effect".
//
// The binding is machine-readable (metric.numerator / .denominator / .guardrails
// / .externalEvents in the frontmatter). `primary` stays a human label. Events
// are resolved against the generated telemetry-contract (sites-emitted events)
// plus the story's own externalEvents (backend-worker-emitted) plus the implicit
// experiment_exposed denominator.
//
// Severity is status-aware: a shipping story (status != draft) with an
// unresolved/absent binding FAILS the build; a draft is reported as backlog and
// does not fail (its events may not be wired yet).

const SITES = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const STORIES_DIR = path.join(SITES, "packages", "features", "src", "stories");
const CONTRACT_PATH = path.join(
  SITES,
  "packages",
  "core",
  "src",
  "lib",
  "telemetry",
  "telemetry-contract.json",
);

const contract = JSON.parse(fs.readFileSync(CONTRACT_PATH, "utf8")) as {
  events: Record<string, unknown>;
};
const CONTRACT_EVENTS = new Set(Object.keys(contract.events));

const RATE_SUFFIXES = ["_rate", "_ctr"];
const TOKEN_RE = /[a-z][a-z0-9]*(?:_[a-z0-9]+)+/g;

function findStories(dir: string, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) findStories(p, out);
    else if (e.name === "story.md") out.push(p);
  }
  return out;
}

type StoryReport = {
  file: string;
  status: string;
  bound: boolean;
  unresolved: string[];
};

const shipping: StoryReport[] = [];
const draftBacklog: StoryReport[] = [];

// exp_key end-to-end: the emit side tags every event with the story's
// experiment.key (exp_key) and the readout joins on it. A duplicate key mixes
// two experiments' data; a key that appears in no code file is emitted by
// nothing, so its readout reads zero. Collect (key -> stories) and (key ->
// status) to check both after the loop.
const expKeyStories = new Map<string, string[]>();
const expKeyStatus = new Map<string, string>();

for (const file of findStories(STORIES_DIR)) {
  let data: Record<string, unknown>;
  try {
    data = matter(fs.readFileSync(file, "utf8")).data;
  } catch {
    continue;
  }
  const metric = data?.metric as
    | {
        primary?: unknown;
        numerator?: unknown;
        denominator?: unknown;
        guardrails?: unknown;
        externalEvents?: unknown;
      }
    | undefined;
  if (!metric) continue;
  const status = typeof data.status === "string" ? data.status : "draft";
  const rel = path.relative(SITES, file);

  const experiment = data?.experiment as { key?: unknown } | undefined;
  const expKey = typeof experiment?.key === "string" ? experiment.key : "";
  if (expKey) {
    if (!expKeyStories.has(expKey)) expKeyStories.set(expKey, []);
    expKeyStories.get(expKey)!.push(rel);
    // Record the "most shipping" status seen for this key.
    const prior = expKeyStatus.get(expKey);
    if (!prior || status === "live" || status === "running") expKeyStatus.set(expKey, status);
  }

  const external = new Set(
    (Array.isArray(metric.externalEvents) ? metric.externalEvents : []).filter(
      (e): e is string => typeof e === "string",
    ),
  );
  const valid = (e: string): boolean =>
    CONTRACT_EVENTS.has(e) || external.has(e) || e === "experiment_exposed";
  const resolves = (tok: string): boolean => {
    if (valid(tok)) return true;
    for (const s of RATE_SUFFIXES) {
      if (tok.endsWith(s) && valid(tok.slice(0, -s.length))) return true;
    }
    return false;
  };

  const bound = typeof metric.numerator === "string" && metric.numerator.length > 0;
  const unresolved: string[] = [];

  if (bound) {
    const events: string[] = [metric.numerator as string];
    if (typeof metric.denominator === "string") events.push(metric.denominator);
    for (const g of Array.isArray(metric.guardrails) ? metric.guardrails : []) {
      if (typeof g === "string") events.push(g);
    }
    for (const e of events) if (!resolves(e)) unresolved.push(e);
  } else {
    // Unbound story: no machine binding yet. Fall back to scanning the label +
    // guardrails for event-shaped tokens so a shipping story can't ship unbound.
    const scan: string[] = [];
    if (typeof metric.primary === "string") scan.push(metric.primary);
    for (const g of Array.isArray(metric.guardrails) ? metric.guardrails : []) {
      if (typeof g === "string") scan.push(g);
    }
    for (const s of scan) {
      for (const tok of s.match(TOKEN_RE) ?? []) {
        if (!resolves(tok)) unresolved.push(tok);
      }
    }
  }

  // Only actively-collecting statuses are gate-enforced; draft/spec are pre-ship
  // and land in the (non-fatal) backlog until they go live. A shipping story
  // must be explicitly bound; a draft only needs its refs to resolve (an
  // unbound draft whose label still resolves via the strip is fine).
  const shippingStatus = status === "live" || status === "running";
  const fails = shippingStatus && (!bound || unresolved.length > 0);
  const backlog = !shippingStatus && unresolved.length > 0;
  if (fails) shipping.push({ file: rel, status, bound, unresolved });
  else if (backlog) draftBacklog.push({ file: rel, status, bound, unresolved });
}

// --- exp_key end-to-end checks ---
const expKeyFailures: string[] = [];
function keyInCode(key: string): boolean {
  try {
    // key is a validated slug below, so single-quoting is injection-safe.
    execSync(`grep -rlF '"${key}"' packages --include='*.ts' --include='*.tsx'`, {
      cwd: SITES,
      stdio: ["ignore", "ignore", "ignore"],
    });
    return true;
  } catch {
    return false; // grep exits 1 on no match
  }
}
for (const [key, stories] of expKeyStories) {
  if (!/^[a-z][a-z0-9_-]*$/.test(key)) {
    expKeyFailures.push(`exp_key "${key}" (${stories[0]}) is not a lowercase slug`);
    continue;
  }
  if (stories.length > 1) {
    expKeyFailures.push(
      `exp_key "${key}" is shared by ${stories.length} stories (${stories.join(", ")}) \u{2014} their telemetry would mix`,
    );
  }
  const status = expKeyStatus.get(key);
  if ((status === "live" || status === "running") && !keyInCode(key)) {
    expKeyFailures.push(
      `exp_key "${key}" (${status}) appears in no code file \u{2014} nothing emits it, so its readout joins on a key that never ships`,
    );
  }
}

if (draftBacklog.length > 0) {
  console.error(`\n${draftBacklog.length} DRAFT story(ies) with an unbound/unresolved metric (backlog, non-fatal):`);
  for (const r of draftBacklog) {
    const why = !r.bound ? "no numerator binding" : `unknown events: ${[...new Set(r.unresolved)].join(", ")}`;
    console.error(`  ${r.file} \u{2014} ${why}`);
  }
}

if (expKeyFailures.length > 0) {
  console.error(`\n${expKeyFailures.length} exp_key problem(s):`);
  for (const f of expKeyFailures) console.error(`  ${f}`);
}

if (shipping.length > 0) {
  console.error(`\n${shipping.length} SHIPPING story(ies) (status != draft) with a broken metric binding:`);
  for (const r of shipping) {
    const why = !r.bound ? "no numerator binding" : `unknown events: ${[...new Set(r.unresolved)].join(", ")}`;
    console.error(`  ${r.file} [${r.status}] \u{2014} ${why}`);
  }
  console.error(
    "\nBind metric.numerator/denominator/guardrails to registered events (see telemetry-contract.json), or list backend-emitted ones in metric.externalEvents.",
  );
}

if (shipping.length > 0 || expKeyFailures.length > 0) process.exit(1);

console.log(
  `telemetry metric bindings + exp_keys OK: every shipping story resolves` +
    (draftBacklog.length ? ` (${draftBacklog.length} draft(s) in backlog)` : ""),
);
process.exit(0);
