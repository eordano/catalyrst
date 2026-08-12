import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { it, expect, beforeAll } from "vitest";

import { describeRequiringPg } from "./require-dep";
import { Pool } from "pg";

import { normCdf, twoProportionZTest } from "../../scripts/story-readout";
import { FIXTURE } from "./telemetry-seed";

const READOUT_SQL = fileURLToPath(
  new URL("../../../../umbrella/metabase/experiment-readout.sql", import.meta.url),
);

const d = describeRequiringPg();

function stripToStatement(block: string): string {
  const out: string[] = [];
  let started = false;
  for (const line of block.split("\n")) {
    const t = line.trim();
    if (!started && (t === "" || t.startsWith("--"))) continue;
    started = true;
    out.push(line);
    if (t.endsWith(";")) break;
  }
  return out.join("\n").trim();
}

function extractCard1(): string {
  const raw = readFileSync(READOUT_SQL, "utf8");
  return stripToStatement(raw.split(/^-- @card /m)[0]);
}

function toPositional(sql: string): { text: string; order: string[] } {
  const order: string[] = [];
  const text = sql.replace(/\{\{(\w+)\}\}/g, (_m, name: string) => {
    let idx = order.indexOf(name);
    if (idx === -1) {
      order.push(name);
      idx = order.length - 1;
    }
    return `$${idx + 1}`;
  });
  return { text, order };
}

const C = FIXTURE.control;
const T = FIXTURE.treatment;
const EXPECTED = (() => {
  const z = twoProportionZTest(C.conversions, C.exposures, T.conversions, T.exposures, 0.05);
  const bayes = (s: number, n: number) => {
    const a = s + 1;
    const b = n - s + 1;
    const mean = a / (a + b);
    const variance = (a * b) / ((a + b) * (a + b) * (a + b + 1));
    return { mean, variance };
  };
  const bc = bayes(C.conversions, C.exposures);
  const bt = bayes(T.conversions, T.exposures);
  const pBeats = normCdf((bt.mean - bc.mean) / Math.sqrt(bt.variance + bc.variance));
  return {
    rate: T.conversions / T.exposures,
    controlRate: C.conversions / C.exposures,
    diff: T.conversions / T.exposures - C.conversions / C.exposures,
    z: z.z,
    pValue: z.p,
    significant: z.significant,
    bayesMean: bt.mean,
    bayesControlMean: bc.mean,
    bayesCiLow: Math.max(0, bt.mean - 1.96 * Math.sqrt(bt.variance)),
    bayesCiHigh: Math.min(1, bt.mean + 1.96 * Math.sqrt(bt.variance)),
    pBeats,
  };
})();

d("experiment-readout.sql CARD 1 vs the seeded fixture", () => {
  let pool: Pool;
  let row: Record<string, unknown>;

  beforeAll(async () => {
    const url = process.env.TELEMETRY_DATABASE_URL ?? process.env.CATALYST_DATABASE_URL;
    expect(url).toBeTruthy();
    pool = new Pool({ connectionString: url, max: 2 });

    const { text, order } = toPositional(extractCard1());
    const values = order.map((name) => {
      switch (name) {
        case "exp_key":
          return FIXTURE.expKey;
        case "metric":
          return FIXTURE.metric;
        case "control":
          return C.variant;
        case "alpha":
          return 0.05;
        case "min_sample":
          return 100;
        default:
          throw new Error(`unexpected template var {{${name}}}`);
      }
    });
    const res = await pool.query(text, values);
    expect(res.rows.length).toBe(1);
    row = res.rows[0];
  });

  it("returns the treatment arm with exact exposures/successes/rate", () => {
    expect(row.variant).toBe(T.variant);
    expect(Number(row.n_exposures)).toBe(T.exposures);
    expect(Number(row.successes)).toBe(T.conversions);
    expect(Number(row.rate)).toBeCloseTo(EXPECTED.rate, 12);
    expect(Number(row.control_rate)).toBeCloseTo(EXPECTED.controlRate, 12);
    expect(Number(row.diff)).toBeCloseTo(EXPECTED.diff, 12);
  });

  it("frequentist z / p_value / significant match (clearly significant)", () => {
    expect(Number(row.z)).toBeCloseTo(EXPECTED.z, 9);
    expect(Number(row.p_value)).toBeCloseTo(EXPECTED.pValue, 12);
    expect(row.significant).toBe(true);
    expect(Number(row.z)).toBeGreaterThan(4.5);
    expect(Number(row.p_value)).toBeLessThan(1e-4);
  });

  it("Bayesian posterior means + treatment 95% CrI match", () => {
    expect(Number(row.bayes_mean)).toBeCloseTo(EXPECTED.bayesMean, 12);
    expect(Number(row.bayes_control_mean)).toBeCloseTo(EXPECTED.bayesControlMean, 12);
    expect(Number(row.bayes_ci_low)).toBeCloseTo(EXPECTED.bayesCiLow, 12);
    expect(Number(row.bayes_ci_high)).toBeCloseTo(EXPECTED.bayesCiHigh, 12);
    expect(Number(row.bayes_control_mean)).toBeLessThan(Number(row.bayes_ci_low));
  });

  it("p_beats_control is ~1 and the verdict is SHIP", () => {
    expect(Number(row.p_beats_control)).toBeCloseTo(EXPECTED.pBeats, 9);
    expect(Number(row.p_beats_control)).toBeGreaterThan(0.999);
    expect(row.verdict).toBe("SHIP");
  });

  it("SQL norm_cdf agrees with TS normCdf within 1e-6 (CLI parity)", async () => {
    const zs = [-3, -1.96, -0.5, 0, 0.5, 1.0, 1.96, 2.5, 4.75];
    const res = await pool.query<{ z: string; cdf: string }>(
      `SELECT z, ext.norm_cdf(z) AS cdf FROM unnest($1::float8[]) AS z`,
      [zs],
    );
    for (const r of res.rows) {
      expect(Number(r.cdf)).toBeCloseTo(normCdf(Number(r.z)), 6);
    }
  });

  it("SQL two_prop_z/p agree with TS twoProportionZTest within 1e-6 (CLI parity)", async () => {
    const cases: Array<[number, number, number, number]> = [
      [C.conversions, C.exposures, T.conversions, T.exposures],
      [10, 100, 12, 100],
      [50, 200, 50, 200],
    ];
    for (const [x1, n1, x2, n2] of cases) {
      const ts = twoProportionZTest(x1, n1, x2, n2, 0.05);
      const res = await pool.query<{ z: string; p: string }>(
        `SELECT ext.two_prop_z($1,$2,$3,$4) AS z, ext.two_prop_p($1,$2,$3,$4) AS p`,
        [x1, n1, x2, n2],
      );
      expect(Number(res.rows[0].z)).toBeCloseTo(ts.z, 6);
      expect(Number(res.rows[0].p)).toBeCloseTo(ts.p, 6);
    }
  });

  it("the supporting rate_by_variant card returns both arms with right rates", async () => {
    const raw = readFileSync(READOUT_SQL, "utf8");
    // `.*$`, not `$`: the marker carries the Metabase chart type after the card
    // name — the real line is `-- @card rate_by_variant bar`. Anchoring straight
    // after the name matched nothing, so `block` was undefined and this test died
    // in stripToStatement rather than asserting anything about the SQL.
    const block = raw.split(/^-- @card rate_by_variant.*$/m)[1];
    if (block === undefined) {
      throw new Error(
        "no `-- @card rate_by_variant` block in experiment-readout.sql; the card was " +
          "renamed or removed, and this test asserts nothing without it",
      );
    }
    const { text, order } = toPositional(stripToStatement(block));
    const values = order.map((n) => (n === "exp_key" ? FIXTURE.expKey : FIXTURE.metric));
    const res = await pool.query(text, values);
    const byVariant = new Map(res.rows.map((r) => [r.variant, r]));
    expect(Number(byVariant.get("control")!.rate)).toBeCloseTo(0.2, 12);
    expect(Number(byVariant.get("treatment")!.rate)).toBeCloseTo(0.35, 12);
    expect(Number(byVariant.get("control")!.exposures)).toBe(400);
    expect(Number(byVariant.get("treatment")!.successes)).toBe(140);
  });

  afterAllSafe(() => pool?.end());
});

import { afterAll } from "vitest";
function afterAllSafe(fn: () => unknown) {
  afterAll(async () => {
    try {
      await fn();
    } catch {
    }
  });
}
