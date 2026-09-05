import path from "node:path";
import { fileURLToPath } from "node:url";

import { parseStory, type StoryMeta } from "../packages/core/src/lib/experiments/context";
import { normInv } from "./sample-size";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const STORIES_DIR = path.resolve(HERE, "../packages/features/src/stories");

export function normCdf(z: number): number {
  const t = 1 / (1 + 0.2316419 * Math.abs(z));
  const d = 0.3989422804014327 * Math.exp(-(z * z) / 2);
  const p =
    d *
    t *
    (0.31938153 +
      t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
  return z >= 0 ? 1 - p : p;
}

export type ZTestResult = {
  z: number;
  p: number;
  diff: number;
  significant: boolean;
};

export function twoProportionZTest(
  x1: number,
  n1: number,
  x2: number,
  n2: number,
  alpha = 0.05,
): ZTestResult {
  const safe = (n: number) => (n > 0 ? n : 0);
  n1 = safe(n1);
  n2 = safe(n2);
  const p1 = n1 > 0 ? x1 / n1 : 0;
  const p2 = n2 > 0 ? x2 / n2 : 0;
  const diff = p2 - p1;

  if (n1 === 0 || n2 === 0) {
    return { z: 0, p: 1, diff, significant: false };
  }
  const pPool = (x1 + x2) / (n1 + n2);
  const se = Math.sqrt(pPool * (1 - pPool) * (1 / n1 + 1 / n2));
  if (!(se > 0)) {
    return { z: 0, p: 1, diff, significant: false };
  }
  const z = diff / se;
  const p = 2 * (1 - normCdf(Math.abs(z)));
  const zCrit = normInv(1 - alpha / 2);
  return { z, p, diff, significant: Math.abs(z) >= zCrit };
}

function env(name: string): string | undefined {
  try {
    return typeof process !== "undefined" ? process.env?.[name] : undefined;
  } catch {
    return undefined;
  }
}

type Counts = Record<string, Record<string, number>>;

function isRate(metric: string): boolean {
  return metric.endsWith("_rate");
}

function rateNumerator(metric: string): string {
  return isRate(metric) ? metric.replace(/_rate$/, "") : metric;
}

const EXPOSURE_EVENT = "experiment_exposed";

// Prefer the machine-readable event binding (metric.numerator/.denominator) over
// stripping the human `primary` label -- the label is prose (verb stems, `_ctr`,
// ratios) and does not reliably name an event. Falls back to the strip for
// stories not yet bound.
function metricNumerator(story: StoryMeta): string {
  return story.metric.numerator ?? rateNumerator(story.metric.primary);
}
function metricDenominator(story: StoryMeta): string {
  return story.metric.denominator ?? EXPOSURE_EVENT;
}

type Fetcher = typeof fetch;

function sqlLiteral(value: string): string {
  if (value.includes(";")) {
    throw new Error(`refusing to interpolate value containing ';': ${value}`);
  }
  return `'${value.replace(/'/g, "''")}'`;
}

export function buildCountsSql(expKey: string, events: string[]): string {
  const inList = events.map(sqlLiteral).join(", ");
  return (
    "SELECT body->'properties'->>'variant' AS variant, " +
    "body->>'event' AS event, count(*) AS c " +
    "FROM telemetry.telemetry_events " +
    "WHERE source = 'segment' " +
    `AND body->'properties'->>'exp_key' = ${sqlLiteral(expKey)} ` +
    `AND body->>'event' IN (${inList}) ` +
    "GROUP BY 1, 2"
  );
}

export function mapSqlRowsToCounts(rows: Array<Record<string, unknown>>): Counts {
  const counts: Counts = {};
  for (const row of rows) {
    const variant = String(row.variant ?? "");
    const event = String(row.event ?? "");
    if (!variant || !event) continue;
    const c = Number(row.c);
    (counts[variant] ??= {})[event] = Number.isFinite(c) ? c : 0;
  }
  return counts;
}

async function fetchSqlCounts(
  base: string,
  expKey: string,
  events: string[],
  doFetch: Fetcher,
): Promise<Counts | null> {
  let sql: string;
  try {
    sql = buildCountsSql(expKey, events);
  } catch {
    return null;
  }
  let res: Response;
  try {
    // /dash/sql sits behind the telemetry admin gate; without the token the
    // service answers 403 and the readout reports "unreachable".
    const token = env("TELEMETRY_ADMIN_TOKEN");
    res = await doFetch(`${base}/dash/sql`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        accept: "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify({ sql }),
    });
  } catch {
    return null;
  }
  if (!res.ok) return null;
  let payload: unknown;
  try {
    payload = await res.json();
  } catch {
    return null;
  }
  const rows = (payload as { rows?: unknown })?.rows;
  if (!Array.isArray(rows)) return null;
  return mapSqlRowsToCounts(rows as Array<Record<string, unknown>>);
}

async function collectCounts(
  story: StoryMeta,
  doFetch: Fetcher,
): Promise<{ counts: Counts; source: "sql" } | null> {
  const base = env("TELEMETRY_URL")?.replace(/\/+$/, "");
  if (!base) return null;

  const expKey = story.experiment.key;
  const needed = new Set<string>();
  needed.add(EXPOSURE_EVENT);
  needed.add(metricNumerator(story));
  needed.add(metricDenominator(story));
  for (const g of story.metric.guardrails) needed.add(rateNumerator(g));

  const events = [...needed];
  const counts = await fetchSqlCounts(base, expKey, events, doFetch);
  if (counts) return { counts, source: "sql" };
  return null;
}

type VariantRow = {
  variant: string;
  num: number;
  den: number;
  rate: number;
  guardrails: Record<string, number>;
};

function deriveRate(counts: Counts, variant: string, event: string): number {
  return counts[variant]?.[event] ?? 0;
}

function computeRows(story: StoryMeta, counts: Counts): VariantRow[] {
  const numEvent = metricNumerator(story);
  const denEvent = metricDenominator(story);

  return story.experiment.variants.map((v) => {
    const exposures = deriveRate(counts, v.id, EXPOSURE_EVENT);
    const num = deriveRate(counts, v.id, numEvent);
    const den = deriveRate(counts, v.id, denEvent);
    const guardrails: Record<string, number> = {};
    for (const g of story.metric.guardrails) {
      if (isRate(g)) {
        const gn = deriveRate(counts, v.id, rateNumerator(g));
        guardrails[g] = exposures > 0 ? gn / exposures : 0;
      } else {
        guardrails[g] = deriveRate(counts, v.id, g);
      }
    }
    return {
      variant: v.id,
      num,
      den,
      rate: den > 0 ? num / den : 0,
      guardrails,
    };
  });
}

type Verdict = "SHIP" | "KILL" | "KEEP RUNNING";

type Decision = {
  verdict: Verdict;
  reasons: string[];
};

function decide(
  story: StoryMeta,
  rows: VariantRow[],
  alpha: number,
): Decision & { tests: Array<{ variant: string } & ZTestResult> } {
  const reasons: string[] = [];
  const minSample = story.experiment.min_sample ?? 0;

  const underpowered = rows.filter((r) => r.den < minSample);
  if (minSample > 0 && underpowered.length > 0) {
    for (const r of underpowered) {
      reasons.push(
        `arm "${r.variant}" has ${r.den} trials < min_sample ${minSample} (underpowered)`,
      );
    }
    return { verdict: "KEEP RUNNING", reasons, tests: [] };
  }

  const control =
    rows.find((r) => r.variant === "control") ??
    (rows.length === 1 ? undefined : rows[0]);

  if (!control || rows.length < 2) {
    reasons.push(
      "no control arm to test against (single-variant story) \u{2014} cannot ship/kill",
    );
    return { verdict: "KEEP RUNNING", reasons, tests: [] };
  }

  const treatments = rows.filter((r) => r.variant !== control.variant);
  const tests = treatments.map((t) => ({
    variant: t.variant,
    ...twoProportionZTest(control.num, control.den, t.num, t.den, alpha),
  }));

  const winner = tests.find((t) => t.significant && t.diff > 0);
  const loser = tests.find((t) => t.significant && t.diff < 0);

  if (winner) {
    reasons.push(
      `treatment "${winner.variant}" beats control by ${(winner.diff * 100).toFixed(2)}pp ` +
        `(z=${winner.z.toFixed(2)}, p=${winner.p.toFixed(4)} < \u{3B1}=${alpha})`,
    );
    return { verdict: "SHIP", reasons, tests };
  }
  if (loser) {
    reasons.push(
      `treatment "${loser.variant}" is WORSE than control by ` +
        `${(loser.diff * 100).toFixed(2)}pp (z=${loser.z.toFixed(2)}, p=${loser.p.toFixed(4)})`,
    );
    return { verdict: "KILL", reasons, tests };
  }
  reasons.push(
    `no treatment moved the primary significantly at \u{3B1}=${alpha} ` +
      `(min p=${Math.min(...tests.map((t) => t.p)).toFixed(4)})`,
  );
  return { verdict: "KEEP RUNNING", reasons, tests };
}

type ReadoutArgs = { id: string; alpha: number; json: boolean };

function parseArgs(argv: string[]): ReadoutArgs {
  const flags: Record<string, string | true> = {};
  const positional: string[] = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const key = a.slice(2);
      const next = argv[i + 1];
      if (next !== undefined && !next.startsWith("--")) {
        flags[key] = next;
        i++;
      } else {
        flags[key] = true;
      }
    } else positional.push(a);
  }
  const id = (flags.id as string) ?? positional[0];
  if (!id) throw new Error("missing <id> (the story slug)");
  const alphaRaw = flags.alpha;
  const alpha = typeof alphaRaw === "string" ? Number(alphaRaw) : 0.05;
  return {
    id,
    alpha: Number.isFinite(alpha) && alpha > 0 && alpha < 1 ? alpha : 0.05,
    json: flags.json === true || flags.json === "true",
  };
}

function pct(n: number): string {
  return `${(n * 100).toFixed(2)}%`;
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  const storyDir = path.join(STORIES_DIR, args.id);
  const story = parseStory(storyDir);

  if (!env("TELEMETRY_URL")) {
    const msg =
      `TELEMETRY_URL is not set \u{2014} cannot read experiment data for "${args.id}".\n` +
      `Set TELEMETRY_URL to your catalyrst-telemetry base URL (and\n` +
      `TELEMETRY_ADMIN_TOKEN to its admin token -- /dash/sql is gated) and re-run, e.g.\n` +
      `  TELEMETRY_URL=https://telemetry.example.com TELEMETRY_ADMIN_TOKEN=... npm run story:readout -- ${args.id}\n`;
    if (args.json) {
      process.stdout.write(
        JSON.stringify({ id: args.id, error: "TELEMETRY_URL unset" }) + "\n",
      );
    } else {
      process.stderr.write(msg);
    }
    return;
  }

  const collected = await collectCounts(story, fetch);
  if (!collected) {
    const msg =
      `Could not read metrics from catalyrst-telemetry for "${args.id}".\n` +
      `POST /dash/sql was unreachable, denied (set TELEMETRY_ADMIN_TOKEN), or ` +
      `returned no rows. Check TELEMETRY_URL and that the experiment ` +
      `(exp_key=${story.experiment.key}) is emitting events.\n`;
    if (args.json) {
      process.stdout.write(
        JSON.stringify({ id: args.id, error: "telemetry unavailable" }) + "\n",
      );
    } else {
      process.stderr.write(msg);
    }
    return;
  }

  const { counts, source } = collected;
  const rows = computeRows(story, counts);
  const decision = decide(story, rows, args.alpha);

  const report = {
    id: story.id,
    experimentKey: story.experiment.key,
    source,
    alpha: args.alpha,
    minSample: story.experiment.min_sample ?? null,
    primary: story.metric.primary,
    guardrails: story.metric.guardrails,
    variants: rows,
    tests: decision.tests,
    verdict: decision.verdict,
    reasons: decision.reasons,
    decisionRule: story.decision.rule,
  };

  if (args.json) {
    process.stdout.write(JSON.stringify(report) + "\n");
    return;
  }

  const lines: string[] = [];
  lines.push(`Readout \u{2014} ${story.id}  (key=${story.experiment.key}, source=${source})`);
  lines.push(`primary: ${story.metric.primary}   \u{3B1}=${args.alpha}   min_sample=${report.minSample}`);
  lines.push("");
  lines.push("variant       trials   primary    " + story.metric.guardrails.join("  "));
  for (const r of rows) {
    const g = story.metric.guardrails
      .map((name) => (isRate(name) ? pct(r.guardrails[name]) : String(r.guardrails[name])))
      .join("  ");
    lines.push(
      `${r.variant.padEnd(13)} ${String(r.den).padStart(6)}   ${pct(r.rate).padStart(7)}    ${g}`,
    );
  }
  if (decision.tests.length) {
    lines.push("");
    for (const t of decision.tests) {
      lines.push(
        `vs control \u{2014} ${t.variant}: \u{394}=${(t.diff * 100).toFixed(2)}pp  z=${t.z.toFixed(2)}  ` +
          `p=${t.p.toFixed(4)}  ${t.significant ? "significant" : "n.s."}`,
      );
    }
  }
  lines.push("");
  lines.push(`VERDICT: ${decision.verdict}`);
  for (const r of decision.reasons) lines.push(`  - ${r}`);
  lines.push("");
  lines.push(`decision.rule: ${story.decision.rule}`);

  process.stderr.write(lines.join("\n") + "\n");
  process.stdout.write(JSON.stringify(report) + "\n");
}

function isMain(): boolean {
  try {
    const entry = process.argv[1] ?? "";
    return import.meta.url === `file://${entry}` || entry.endsWith("story-readout.ts");
  } catch {
    return false;
  }
}

if (isMain()) {
  main().catch((err) => {
    process.stderr.write(
      `story:readout error: ${err instanceof Error ? err.message : String(err)}\n` +
        `usage: npm run story:readout -- <id> [--alpha 0.05] [--json]\n`,
    );
    process.exit(1);
  });
}
