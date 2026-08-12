import { createServer, type Server } from "node:http";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { it, expect, beforeAll, afterAll } from "vitest";

import { describeRequiringPg } from "./require-dep";
import { Pool } from "pg";

import { normCdf, twoProportionZTest } from "../../scripts/story-readout";
import { CREATE_FIXTURE } from "./create-telemetry-seed";

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

const ALPHA = 0.0125;
const MIN_SAMPLE = 4000;

async function runCard1(
  pool: Pool,
  bind: { expKey: string; metric: string; control: string; alpha: number; minSample: number },
): Promise<Array<Record<string, unknown>>> {
  const { text, order } = toPositional(extractCard1());
  const values = order.map((name) => {
    switch (name) {
      case "exp_key":
        return bind.expKey;
      case "metric":
        return bind.metric;
      case "control":
        return bind.control;
      case "alpha":
        return bind.alpha;
      case "min_sample":
        return bind.minSample;
      default:
        throw new Error(`unexpected template var {{${name}}}`);
    }
  });
  const res = await pool.query(text, values);
  return res.rows as Array<Record<string, unknown>>;
}

d("create-entry-preview CARD 1 (seeded fixture, Bonferroni alpha)", () => {
  let pool: Pool;
  let byVariant: Map<string, Record<string, unknown>>;

  beforeAll(async () => {
    const url = process.env.TELEMETRY_DATABASE_URL ?? process.env.CATALYST_DATABASE_URL;
    expect(url).toBeTruthy();
    pool = new Pool({ connectionString: url, max: 2 });
    const rows = await runCard1(pool, {
      expKey: CREATE_FIXTURE.expKey,
      metric: CREATE_FIXTURE.metric,
      control: CREATE_FIXTURE.control.variant,
      alpha: ALPHA,
      minSample: MIN_SAMPLE,
    });
    byVariant = new Map(rows.map((r) => [String(r.variant), r]));
    expect(rows.length).toBe(4);
    expect(rows.map((r) => r.variant)).toEqual([
      "builder-or-download",
      "capability-routed",
      "download-hub",
      "hub-or-download",
    ]);
  });

  const C = CREATE_FIXTURE.control;

  for (const arm of CREATE_FIXTURE.treatments) {
    it(`${arm.variant}: exposures/successes/rate/diff + verdict ${arm.verdict}`, () => {
      const row = byVariant.get(arm.variant)!;
      const ts = twoProportionZTest(C.conversions, C.exposures, arm.conversions, arm.exposures, ALPHA);

      expect(Number(row.n_exposures)).toBe(arm.exposures);
      expect(Number(row.successes)).toBe(arm.conversions);
      expect(Number(row.rate)).toBeCloseTo(arm.conversions / arm.exposures, 12);
      expect(Number(row.control_rate)).toBeCloseTo(0.25, 12);
      expect(Number(row.diff)).toBeCloseTo(arm.conversions / arm.exposures - 0.25, 12);
      expect(Number(row.z)).toBeCloseTo(ts.z, 9);

      expect(row.verdict).toBe(arm.verdict);

      if (arm.variant === "builder-or-download" || arm.variant === "download-hub") {
        expect(row.significant).toBe(true);
        expect(Number(row.p_value)).toBeLessThan(1e-4);
      } else {
        expect(Number(row.p_value)).toBeCloseTo(ts.p, 9);
        expect(row.significant).toBe(arm.verdict !== "KEEP RUNNING");
      }
    });
  }

  it("ships the BEST arm: builder-or-download is the highest-rate SHIP arm", () => {
    const ships = CREATE_FIXTURE.treatments
      .map((t) => byVariant.get(t.variant)!)
      .filter((r) => r.verdict === "SHIP");
    expect(ships.length).toBe(2);
    const best = ships.reduce((a, b) => (Number(a.rate) >= Number(b.rate) ? a : b));
    expect(best.variant).toBe("builder-or-download");
    expect(Number(best.rate)).toBeCloseTo(0.33, 12);
  });

  it("SQL z agrees with TS twoProportionZTest for the moderate arms (CLI parity)", () => {
    for (const arm of ["capability-routed", "hub-or-download"]) {
      const t = CREATE_FIXTURE.treatments.find((x) => x.variant === arm)!;
      const ts = twoProportionZTest(C.conversions, C.exposures, t.conversions, t.exposures, ALPHA);
      const row = byVariant.get(arm)!;
      expect(Number(row.z)).toBeCloseTo(ts.z, 6);
      expect(Number(row.p_value)).toBeCloseTo(ts.p, 6);
    }
    expect(normCdf(0)).toBeCloseTo(0.5, 6);
  });

  afterAll(async () => {
    try {
      await pool?.end();
    } catch {
    }
  });
});

d("create-entry-preview LIVE track() loop (real emit -> PG -> readout)", () => {
  const LIVE_KEY = "create_entry_preview_live";
  let pool: Pool;
  let server: Server;
  let port = 0;
  let prevTelemetryUrl: string | undefined;
  let track: typeof import("../../packages/core/src/lib/telemetry/track").track;
  let trackExposure: typeof import("../../packages/core/src/lib/telemetry/track").trackExposure;

  const COHORT = [
    { variant: "control", exposures: 10, conversions: 1 },
    { variant: "builder-or-download", exposures: 10, conversions: 8 },
    { variant: "hub-or-download", exposures: 10, conversions: 3 },
  ];
  const EXPECTED_EMISSIONS =
    COHORT.reduce((a, c) => a + c.exposures, 0) +
    COHORT.reduce((a, c) => a + c.conversions, 0);

  beforeAll(async () => {
    const url = process.env.TELEMETRY_DATABASE_URL ?? process.env.CATALYST_DATABASE_URL;
    expect(url).toBeTruthy();
    pool = new Pool({ connectionString: url, max: 2 });

    server = createServer((req, res) => {
      const chunks: Buffer[] = [];
      req.on("data", (c) => chunks.push(c as Buffer));
      req.on("end", () => {
        const raw = Buffer.concat(chunks).toString("utf8");
        res.writeHead(204).end();
        try {
          JSON.parse(raw);
          void pool
            .query(
              `INSERT INTO telemetry.telemetry_events (source, project, event_kind, received_at, body)
               VALUES ('segment','sites','track','2026-06-20T12:00:00Z'::timestamptz, $1::jsonb)`,
              [raw],
            )
            .catch(() => {});
        } catch {
        }
      });
    });
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    const addr = server.address();
    port = typeof addr === "object" && addr ? addr.port : 0;
    expect(port).toBeGreaterThan(0);

    prevTelemetryUrl = process.env.TELEMETRY_URL;
    process.env.TELEMETRY_URL = `http://127.0.0.1:${port}`;

    const mod = await import("../../packages/core/src/lib/telemetry/track");
    track = mod.track;
    trackExposure = mod.trackExposure;
  });

  it("real trackExposure/track emissions land in PG and CARD 1 ships the winner", async () => {
    let sidN = 0;
    for (const arm of COHORT) {
      for (let i = 0; i < arm.exposures; i++) {
        const ctx = {
          sid: `live-${arm.variant}-${sidN++}`,
          story: "create/entry-preview",
          variant: arm.variant,
          experimentKey: LIVE_KEY,
        };
        trackExposure(ctx);
        if (i < arm.conversions) {
          track(
            "create_preview",
            { path: "builder", target: "/create/wearables/item-editor?from=create-entry" },
            ctx,
          );
        }
      }
    }

    const deadline = Date.now() + 3000;
    let landed = 0;
    while (Date.now() < deadline) {
      const r = await pool.query<{ c: string }>(
        `SELECT count(*) AS c FROM telemetry.telemetry_events
         WHERE body->'properties'->>'exp_key' = $1`,
        [LIVE_KEY],
      );
      landed = Number(r.rows[0].c);
      if (landed >= EXPECTED_EMISSIONS) break;
      await new Promise((r) => setTimeout(r, 50));
    }
    expect(landed).toBe(EXPECTED_EMISSIONS);

    const rows = await runCard1(pool, {
      expKey: LIVE_KEY,
      metric: "create_preview_rate",
      control: "control",
      alpha: 0.05,
      minSample: 3,
    });
    expect(rows.length).toBeGreaterThanOrEqual(1);
    const byVariant = new Map(rows.map((r) => [String(r.variant), r]));

    const winner = byVariant.get("builder-or-download")!;
    expect(Number(winner.n_exposures)).toBe(10);
    expect(Number(winner.successes)).toBe(8);
    expect(Number(winner.rate)).toBeCloseTo(0.8, 12);
    expect(Number(winner.control_rate)).toBeCloseTo(0.1, 12);
    expect(winner.significant).toBe(true);
    expect(winner.verdict).toBe("SHIP");

    const flat = byVariant.get("hub-or-download")!;
    expect(flat.verdict).not.toBe("SHIP");
  });

  afterAll(async () => {
    if (prevTelemetryUrl === undefined) delete process.env.TELEMETRY_URL;
    else process.env.TELEMETRY_URL = prevTelemetryUrl;
    await new Promise<void>((resolve) => {
      if (server) server.close(() => resolve());
      else resolve();
    });
    try {
      await pool?.end();
    } catch {
    }
  });
});
