import { describe, it, expect, beforeAll, afterAll } from "vitest";

import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { Pool } from "pg";

import { describeRequiringPg } from "./require-dep";

import { sidBadge } from "../../packages/data/src/lib/foundry/db.server";
import { importReal, loadRealFixture } from "../../packages/data/src/lib/foundry/seed.server";
import {
  appendEvents,
  getTrajectory,
  deriveFinishReason,
  importTrajectorySample,
  type TrajectorySample,
} from "../../packages/data/src/lib/foundry/trajectory.server";

import { loader as layoutLoader } from "../../packages/routes/app/routes/foundry";
import { loader as homeLoader } from "../../packages/routes/app/routes/foundry._index";
import {
  loader as exchangeLoader,
  action as exchangeAction,
} from "../../packages/routes/app/routes/foundry.exchange";
import { loader as playLoader } from "../../packages/routes/app/routes/foundry.play";
import { loader as gameLoader } from "../../packages/routes/app/routes/foundry.play_.$slug";
import { loader as gddListLoader } from "../../packages/routes/app/routes/foundry.gdd";
import { loader as gddDocLoader } from "../../packages/routes/app/routes/foundry.gdd_.$id";
import { loader as copilotLoader } from "../../packages/routes/app/routes/foundry.copilot";
import { loader as benchLoader } from "../../packages/routes/app/routes/foundry.console.bench";
import { loader as trajectoriesLoader } from "../../packages/routes/app/routes/foundry.console.trajectories";
import { loader as replayLoader } from "../../packages/routes/app/routes/foundry.console.trajectories_.$id";
import { loader as costsLoader } from "../../packages/routes/app/routes/foundry.console.costs";

const d = describeRequiringPg();

const BASE = "http://localhost";
const SITES = fileURLToPath(new URL("../../", import.meta.url));
const UI3_FOUNDRY = fileURLToPath(new URL("../../../ui3/src/foundry/", import.meta.url));
const FIXTURES = join(SITES, "packages/data/src/fixtures");
const GDD_FIXTURES = join(FIXTURES, "gdd");

type RouteHandler = (args: never) => unknown;

function get(path: string, sid: string): Request {
  return new Request(`${BASE}${path}`, { headers: { cookie: `sid=${sid}` } });
}

function post(path: string, sid: string, fields: Record<string, string>): Request {
  return new Request(`${BASE}${path}`, {
    method: "POST",
    headers: {
      cookie: `sid=${sid}`,
      "content-type": "application/x-www-form-urlencoded",
    },
    body: new URLSearchParams(fields).toString(),
  });
}

// Loaders and actions return either a plain object or react-router `data()`,
// which wraps the payload so the route can attach headers.
type DataResult = {
  data: Record<string, unknown>;
  init?: { status?: number; headers?: Record<string, string> } | null;
};

function unwrap(result: unknown): Record<string, unknown> {
  if (result && typeof result === "object" && "data" in (result as object)) {
    return (result as DataResult).data;
  }
  return result as Record<string, unknown>;
}

async function callRaw(
  handler: RouteHandler,
  request: Request,
  params: Record<string, string> = {},
): Promise<unknown> {
  return (handler as (a: unknown) => unknown)({ request, params, context: {} });
}

async function call(
  handler: RouteHandler,
  request: Request,
  params: Record<string, string> = {},
): Promise<Record<string, unknown>> {
  return unwrap(await callRaw(handler, request, params));
}

function statusOf(result: unknown): number {
  if (result && typeof result === "object" && "init" in (result as object)) {
    return (result as DataResult).init?.status ?? 200;
  }
  return 200;
}

function setCookieSid(result: unknown): string | null {
  if (!result || typeof result !== "object" || !("init" in (result as object))) {
    return null;
  }
  const headers = (result as DataResult).init?.headers ?? {};
  const raw = headers["Set-Cookie"] ?? headers["set-cookie"];
  const match = raw?.match(/(?:^|;\s*)sid=([^;]+)/);
  return match ? decodeURIComponent(match[1]) : null;
}

// ---------------------------------------------------------------------------
// The fiction regression gate.
//
// v2 shipped a seeded program — invented studios, invented gate verdicts, an
// LCG "bot swarm" — and its names are the cheapest possible tripwire: if any of
// them reappears in a route, a data module, a rendered page or a shipped
// fixture, something fictional came back with it. `pilot` is banned as a word
// on its own (the v2 provenance chip); "copilot" is a real service here and is
// deliberately not matched.
// ---------------------------------------------------------------------------

const FICTION = [
  /Neon Relay/i,
  /Vega Works/i,
  /Squad Volt/i,
  /Static Garden/i,
  /Vault Sprint/i,
  /Mara Ilves/i,
  /Halide/i,
  /(?<![A-Za-z])pilots?(?![A-Za-z])/i,
];

function walk(dir: string, match: RegExp, out: string[] = []): string[] {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const e of entries) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p, match, out);
    else if (match.test(e.name)) out.push(p);
  }
  return out;
}

function foundrySurfaceFiles(): string[] {
  const files = [
    ...walk(join(SITES, "packages/routes/app/routes"), /^foundry.*\.tsx?$/),
    ...walk(join(SITES, "packages/data/src/lib/foundry"), /\.(ts|sql)$/),
    ...walk(UI3_FOUNDRY, /\.(tsx?|css)$/),
  ];
  for (const name of ["foundry.json", "foundry-real.json"]) {
    const p = join(FIXTURES, name);
    try {
      statSync(p);
      files.push(p);
    } catch {
      // a fixture a lane has not written yet is that lane's gate, not this one's
    }
  }
  return files;
}

describe("the foundry surface carries none of v2's fiction", () => {
  const files = foundrySurfaceFiles();

  it("scans a surface that actually exists", () => {
    // A grep gate that silently matched nothing would pass forever.
    expect(files.length).toBeGreaterThan(10);
  });

  it("names no invented studio, scene or person, and never says 'pilot'", () => {
    const hits: string[] = [];
    for (const file of files) {
      const text = readFileSync(file, "utf8");
      for (const re of FICTION) {
        const m = text.match(re);
        if (m) hits.push(`${file.slice(SITES.length)}: ${m[0]}`);
      }
    }
    expect(hits).toEqual([]);
  });

  it("ships the specced empty states in the pages that render them", () => {
    // The routes below are asserted to return zero rows on an empty database;
    // this is the other half of that claim — the copy a visitor then reads.
    // Asserted file by file, not against the concatenated surface: the whole
    // point is that the page a visitor lands on carries its own empty state.
    const owners: [string, string][] = [
      ["pages/FdPlayPage.tsx", "No games imported yet"],
      ["pages/FdBenchPage.tsx", "No recorded bot runs"],
      ["pages/FdCostsPage.tsx", "No copilot usage recorded yet"],
      ["pages/FdCopilotPage.tsx", "No copilot usage recorded yet"],
      ["pages/FdGddListPage.tsx", "No design docs yet"],
      ["pages/FdExchangePage.tsx", "No requests yet"],
      ["pages/FdTrajectoriesPage.tsx", "No episodes recorded"],
    ];
    for (const [file, copy] of owners) {
      expect(readFileSync(join(UI3_FOUNDRY, file), "utf8"), `${file} lost its empty state`).toContain(
        copy,
      );
    }
  });
});

// ---------------------------------------------------------------------------
// Database suites. globalSetup applies the foundry schema and seeds NOTHING, so
// the first suite below is the honest-empty-site proof and the second one earns
// every row it asserts on by importing real artifacts.
// ---------------------------------------------------------------------------

type SinkEvent = {
  event: string;
  anonymousId: string;
  properties: Record<string, unknown>;
};

const sink: SinkEvent[] = [];
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

function sinkEvents(sid: string, name: string): SinkEvent[] {
  return sink.filter((e) => e.event === name && e.anonymousId === sid);
}

// Waits for the expected rows, then keeps waiting a beat so an "exactly one"
// claim would still catch a duplicate arriving late.
async function settledEvents(
  sid: string,
  name: string,
  expected: number,
): Promise<SinkEvent[]> {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline && sinkEvents(sid, name).length < expected) {
    await sleep(25);
  }
  await sleep(300);
  return sinkEvents(sid, name);
}

let telemetrySink: Server;
let telemetryUrlBefore: string | undefined;

async function startSink(): Promise<void> {
  telemetrySink = createServer((req, res) => {
    if (req.method !== "POST") {
      res.writeHead(404, { "content-type": "application/json" });
      res.end("{}");
      return;
    }
    let body = "";
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      try {
        const parsed = JSON.parse(body) as SinkEvent;
        if (parsed && typeof parsed.event === "string") sink.push(parsed);
      } catch {
        // a malformed body is not this sink's problem
      }
      res.writeHead(200, { "content-type": "application/json" });
      res.end("{}");
    });
  });
  await new Promise<void>((resolve) => {
    telemetrySink.listen(0, "127.0.0.1", resolve);
  });
  telemetryUrlBefore = process.env.TELEMETRY_URL;
  const { port } = telemetrySink.address() as AddressInfo;
  process.env.TELEMETRY_URL = `http://127.0.0.1:${port}`;
}

async function stopSink(): Promise<void> {
  if (telemetryUrlBefore === undefined) delete process.env.TELEMETRY_URL;
  else process.env.TELEMETRY_URL = telemetryUrlBefore;
  telemetrySink.closeAllConnections?.();
  await new Promise<void>((resolve) => {
    telemetrySink.close(() => resolve());
  });
}

function connectionUrl(): string {
  const url = process.env.FOUNDRY_DATABASE_URL ?? process.env.CATALYST_DATABASE_URL;
  if (!url) throw new Error("no FOUNDRY_DATABASE_URL: globalSetup did not provision postgres");
  return url;
}

// An empty database is a normal state, not an error: a loader must answer it
// with a payload. The throw is captured rather than rethrown so the assertion
// can name the route, and so the fiction grep still reads whatever came back.
type LoaderAnswer = { body: string; threw: boolean };

async function answerOf(fn: () => Promise<Record<string, unknown>>): Promise<LoaderAnswer> {
  try {
    return { body: JSON.stringify(await fn()), threw: false };
  } catch (err) {
    const body =
      err instanceof Response
        ? `${err.status} ${await err.clone().text()}`
        : err instanceof Error
          ? err.message
          : String(err);
    return { body, threw: true };
  }
}

d("Foundry with an empty database (migrated, nothing imported)", () => {
  let pool: Pool;

  async function count(sql: string, values: unknown[] = []): Promise<number> {
    const res = await pool.query(sql, values);
    return Number(res.rows[0]?.n ?? 0);
  }

  beforeAll(async () => {
    pool = new Pool({ connectionString: connectionUrl(), max: 2 });
    await startSink();
  });

  afterAll(async () => {
    await stopSink();
    await pool.end();
  });

  it("starts from zero rows in every content table", async () => {
    for (const table of [
      "scene",
      "gdd_doc",
      "bot_report",
      "trajectory",
      "llm_usage",
      "request",
    ]) {
      expect(await count(`SELECT count(*)::int AS n FROM foundry.${table}`)).toBe(0);
    }
  });

  it("answers every route without inventing a single row", async () => {
    const sid = "e2e-foundry-empty";
    const answers: Record<string, LoaderAnswer> = {
      "/foundry": await answerOf(() => call(homeLoader as RouteHandler, get("/foundry", sid))),
      "/foundry/play": await answerOf(() =>
        call(playLoader as RouteHandler, get("/foundry/play", sid)),
      ),
      "/foundry/gdd": await answerOf(() =>
        call(gddListLoader as RouteHandler, get("/foundry/gdd", sid)),
      ),
      "/foundry/copilot": await answerOf(() =>
        call(copilotLoader as RouteHandler, get("/foundry/copilot", sid)),
      ),
      "/foundry/exchange": await answerOf(() =>
        call(exchangeLoader as RouteHandler, get("/foundry/exchange", sid)),
      ),
      "/foundry/console/bench": await answerOf(() =>
        call(benchLoader as RouteHandler, get("/foundry/console/bench", sid)),
      ),
      "/foundry/console/trajectories": await answerOf(() =>
        call(trajectoriesLoader as RouteHandler, get("/foundry/console/trajectories", sid)),
      ),
      "/foundry/console/costs": await answerOf(() =>
        call(costsLoader as RouteHandler, get("/foundry/console/costs", sid)),
      ),
    };

    for (const [path, { body, threw }] of Object.entries(answers)) {
      // Zero rows is the shipped state of this site on day one. Every one of
      // these routes has to render it, so none of them may answer with a throw.
      expect(threw, `${path} threw on an empty database: ${body.slice(0, 200)}`).toBe(false);
      expect(body, `${path} produced no payload`).toBeTruthy();
      for (const re of FICTION) {
        expect(re.test(body), `${path} answered with fiction: ${body.slice(0, 200)}`).toBe(
          false,
        );
      }
      expect(body, `${path} reported a database failure`).not.toMatch(
        /database is not configured/i,
      );
    }
  });

  it("returns empty collections, not placeholder ones", async () => {
    const sid = "e2e-foundry-empty-lists";
    const play = await call(playLoader as RouteHandler, get("/foundry/play", sid));
    expect(play.games ?? []).toEqual([]);

    const gdd = await call(gddListLoader as RouteHandler, get("/foundry/gdd", sid));
    expect(gdd.docs ?? []).toEqual([]);

    const bench = await call(benchLoader as RouteHandler, get("/foundry/console/bench", sid));
    expect(bench.reports ?? []).toEqual([]);

    const trajectories = await call(
      trajectoriesLoader as RouteHandler,
      get("/foundry/console/trajectories", sid),
    );
    expect(trajectories.records ?? []).toEqual([]);

    const costs = await call(costsLoader as RouteHandler, get("/foundry/console/costs", sid));
    const usage = costs.usage as { messages: number; costUsd: number } | undefined;
    expect(usage?.messages ?? 0).toBe(0);
    expect(usage?.costUsd ?? 0).toBe(0);

    const exchange = await call(exchangeLoader as RouteHandler, get("/foundry/exchange", sid));
    expect(exchange.requests ?? []).toEqual([]);
  });

  it("a missing game is a 404, not an invented one", async () => {
    let status = 200;
    try {
      const result = await callRaw(
        gameLoader as RouteHandler,
        get("/foundry/play/not-a-game", "e2e-foundry-404"),
        { slug: "not-a-game" },
      );
      status = result instanceof Response ? result.status : statusOf(result);
    } catch (err) {
      status = err instanceof Response ? err.status : statusOf(err);
    }
    expect(status).toBe(404);
  });

  it("one document request mints one sid, however many loaders read it", async () => {
    const paths: [string, RouteHandler[]][] = [
      ["/foundry", [layoutLoader as RouteHandler, homeLoader as RouteHandler]],
      ["/foundry/console/costs", [layoutLoader as RouteHandler, costsLoader as RouteHandler]],
    ];

    for (const [path, loaders] of paths) {
      // One cookieless Request, shared by the layout and its leaf exactly as the
      // server shares it: the parallel loaders must not race to mint a sid each.
      const request = new Request(`${BASE}${path}`);
      const results = await Promise.all(loaders.map((l) => callRaw(l, request)));
      const minted = results.map(setCookieSid).filter((s): s is string => s !== null);
      expect(minted.length).toBeGreaterThan(0);
      expect(new Set(minted).size).toBe(1);
    }
  });

  it("a front-door visit is exposed once, tour navigation included", async () => {
    const sid = "e2e-foundry-exposure";
    await call(homeLoader as RouteHandler, get("/foundry", sid));
    // The auto-tour arm replaces the URL with its own params; that is the same
    // visit, not a second sample.
    await call(homeLoader as RouteHandler, get("/foundry?tour=1&tour_src=auto", sid));

    const exposures = await settledEvents(sid, "experiment_exposed", 1);
    expect(exposures.length).toBe(1);
    expect(exposures[0].properties.exp_key).toBe("foundry-tour-activation");
  });
});

d("Foundry with the real imports applied", () => {
  let pool: Pool;
  const sids = {
    a: "e2e-foundry-a",
    b: "e2e-foundry-b",
    author: "e2e-foundry-author",
    cap: "e2e-foundry-cap",
  };

  async function count(sql: string, values: unknown[] = []): Promise<number> {
    const res = await pool.query(sql, values);
    return Number(res.rows[0]?.n ?? 0);
  }

  function runScript(script: string, args: string[]): void {
    execFileSync(join(SITES, "node_modules/.bin/tsx"), [join(SITES, script), ...args], {
      cwd: SITES,
      stdio: "pipe",
      timeout: 120_000,
    });
  }

  beforeAll(async () => {
    pool = new Pool({ connectionString: connectionUrl(), max: 2 });
    await startSink();
  });

  afterAll(async () => {
    await stopSink();
    await pool.end();
  });

  it("imports the eight real games, dates and all", async () => {
    const fixture = loadRealFixture();
    await importReal(pool, fixture);
    // Idempotent: the import is re-run by the deploy loop, not once ever.
    await importReal(pool, fixture);

    expect(await count("SELECT count(*)::int AS n FROM foundry.scene")).toBe(
      fixture.scenes.length,
    );
    expect(fixture.scenes.length).toBe(8);

    const flagtag = await pool.query(
      `SELECT title, world_name, entity_id, source, parcels, size_bytes,
              to_char(deployed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS day
         FROM foundry.scene WHERE id = 'flagtag'`,
    );
    expect(flagtag.rows[0]?.title).toBe("Flag Tag");
    expect(flagtag.rows[0]?.world_name).toBe("flagtag.dcl.eth");
    expect(flagtag.rows[0]?.day).toBe("2026-07-05");
    expect(flagtag.rows[0]?.source).toBe("worlds-mirror");
    // Row equals fixture, exactly: the import transforms nothing on the way in,
    // so the mirror's parcel count and byte size are what a visitor is shown.
    const fromFixture = fixture.scenes.find((s) => s.id === "flagtag")!;
    expect(Number(flagtag.rows[0]?.parcels)).toBe(fromFixture.parcels);
    expect(Number(flagtag.rows[0]?.size_bytes)).toBe(fromFixture.sizeBytes);
    expect(flagtag.rows[0]?.entity_id).toBe(fromFixture.entityId);

    // The template scene is the one row that is not a Worlds deployment, and it
    // carries no deployment date rather than a borrowed one.
    const template = await pool.query(
      "SELECT source, deployed_at, world_name FROM foundry.scene WHERE id = 'template-game'",
    );
    expect(template.rows[0]?.source).toBe("repo");
    expect(template.rows[0]?.deployed_at).toBeNull();
    expect(template.rows[0]?.world_name).toBeNull();

    // One changelog row per real deployment; the repo scene contributes none,
    // and the second (idempotent) import added no duplicates.
    expect(await count("SELECT count(*)::int AS n FROM foundry.scene_changelog")).toBe(
      fixture.scenes.filter((s) => s.deployedAt !== null).length,
    );
  });

  it("the play surface shows the imported games and their provenance", async () => {
    const play = await call(playLoader as RouteHandler, get("/foundry/play", sids.a));
    const games = play.games as { slug: string; title: string; sourceNote: string }[];
    expect(games.length).toBe(8);
    const card = games.find((g) => g.slug === "flagtag")!;
    expect(card.title).toBe("Flag Tag");
    // Every card can say where it came from.
    expect(card.sourceNote.length).toBeGreaterThan(0);

    const detail = await call(
      gameLoader as RouteHandler,
      get("/foundry/play/flagtag", sids.a),
      { slug: "flagtag" },
    );
    const scene = detail.game as { worldName: string; deployedAt: string };
    expect(scene.worldName).toBe("flagtag.dcl.eth");
    expect(scene.deployedAt?.slice(0, 10)).toBe("2026-07-05");
    // The embed is gated on a real probe; whatever it answered, it answered
    // with a boolean and a timestamp rather than an assumption.
    const embed = detail.embed as { reachable: boolean; probedAt: string } | null;
    if (embed) {
      expect(typeof embed.reachable).toBe("boolean");
      expect(Date.parse(embed.probedAt)).not.toBeNaN();
    }
  });

  it("imports the real shortGDDs and parses their honesty state from the markdown", async () => {
    const docs = readdirSync(GDD_FIXTURES)
      .filter((f) => f.endsWith(".md"))
      .sort()
      .map((f) => join(GDD_FIXTURES, f));
    expect(docs.length).toBeGreaterThanOrEqual(3);

    runScript("scripts/foundry-import-gdd.mts", [...docs, "--db", connectionUrl()]);

    expect(await count("SELECT count(*)::int AS n FROM foundry.gdd_doc")).toBe(docs.length);

    const list = await call(gddListLoader as RouteHandler, get("/foundry/gdd", sids.a));
    const rows = list.docs as { id: string; title: string }[];
    expect(rows.length).toBe(docs.length);

    const pixelwars = rows.find((r) => /pixelwars/i.test(r.id) && !/v2/.test(r.id));
    expect(pixelwars, "the Pixelwars shortGDD did not import").toBeTruthy();

    const doc = await call(
      gddDocLoader as RouteHandler,
      get(`/foundry/gdd/${pixelwars!.id}`, sids.a),
      { id: pixelwars!.id },
    );
    const body = doc.doc as {
      honesty: { totals: Record<string, number>; sections: unknown[] };
      hypotheses: { id: string; status: string }[];
      bodyMd: string;
    };
    // The real document: TBD and [HYPOTHESIS] markers throughout, zero [OPEN]
    // sections, and a hypothesis log twelve files deep. The assertion is on the
    // parse of the shipped markdown, never on numbers typed in here.
    expect(body.honesty.sections.length).toBeGreaterThan(5);
    expect(body.honesty.totals.tbd).toBeGreaterThan(0);
    expect(body.honesty.totals.hypothesis).toBeGreaterThan(0);
    expect(body.honesty.totals.open).toBe(0);
    expect(body.hypotheses.length).toBe(12);
    expect(body.hypotheses.every((h) => h.status === "parked")).toBe(true);
    expect(body.bodyMd).toContain("Pixelwars");

    // The review pass is a second version, linked rather than overwriting.
    const superseded = await pool.query(
      "SELECT id, supersedes, version FROM foundry.gdd_doc WHERE supersedes IS NOT NULL",
    );
    expect(superseded.rowCount).toBeGreaterThanOrEqual(1);
    expect(Number(superseded.rows[0].version)).toBeGreaterThan(1);
  });

  it("replays a real arena episode from its own event log", async () => {
    // Captured from `python3 -m dclbots.arena --seed 7`: one obs/snapshot per
    // printed line, verbatim, bracketed by the turn it belongs to.
    const sample = JSON.parse(
      readFileSync(join(FIXTURES, "trajectory-arena-sample.json"), "utf8"),
    ) as TrajectorySample;
    expect(sample.events.length, "the arena sample carries no events").toBeGreaterThan(2);
    expect(sample.generatedFrom.command).toContain("dclbots.arena");

    const id = await importTrajectorySample(pool, sample);
    // Idempotent: ingesting the same evidence twice is one episode, not two.
    await importTrajectorySample(pool, sample);

    // Contiguity is the whole contract: an appended gap is a corrupted replay,
    // so it must be refused rather than stored.
    await expect(
      appendEvents(pool, id, [
        {
          seq: sample.events.length + 5,
          type: "obs/snapshot",
          time: new Date().toISOString(),
          data: {},
        },
      ]),
    ).rejects.toThrow();

    const stored = await getTrajectory(pool, id);
    expect(stored).toBeTruthy();
    expect(stored!.events.length).toBe(sample.events.length);
    expect(stored!.events.map((e) => e.seq)).toEqual(stored!.events.map((_, i) => i));
    const last = stored!.events[stored!.events.length - 1];
    expect(last.type).toBe("turn/end");
    expect(deriveFinishReason(stored!.events)?.kind).toBe("completed");
    // Every printed line survived the round trip byte for byte.
    const lines = stored!.events.filter((e) => e.type === "obs/snapshot").length;
    expect(lines).toBe(
      sample.events.filter((e) => e.type === "obs/snapshot").length,
    );

    const list = await call(
      trajectoriesLoader as RouteHandler,
      get("/foundry/console/trajectories", sids.a),
    );
    const records = list.records as { id: string; events: number }[];
    const mine = records.find((r) => r.id === id);
    expect(mine).toBeTruthy();
    expect(mine!.events).toBe(sample.events.length);

    const replay = await call(
      replayLoader as RouteHandler,
      get(`/foundry/console/trajectories/${id}`, sids.a),
      { id },
    );
    const events = replay.events as { seq: number; type: string }[];
    expect(events.length).toBe(sample.events.length);
    expect(events[0].type).toBe("turn/start");
  });

  it("costs are summed from measured tokens and labeled reference pricing", async () => {
    // The row below is the one exchange this program actually measured against
    // its own gateway: 9623 input tokens, 76 output, and the cost the gateway
    // itself reported. Nothing about it is estimated.
    await pool.query(
      `INSERT INTO foundry.llm_usage
         (message_id, session_id, session_title, model, input_tokens, output_tokens,
          cost_usd, price_input_per_m, price_output_per_m, at)
       VALUES ('e2e-msg-1', 'e2e-ses-1', 'gateway metering probe', 'decent/llm-default',
               9623, 76, 0.0009851, 0.10, 0.30, now())
       ON CONFLICT (message_id) DO NOTHING`,
    );

    const costs = await call(costsLoader as RouteHandler, get("/foundry/console/costs", sids.a));
    const usage = costs.usage as {
      messages: number;
      inputTokens: number;
      outputTokens: number;
      costUsd: number;
    };
    expect(usage.messages).toBe(1);
    expect(usage.inputTokens).toBe(9623);
    expect(usage.outputTokens).toBe(76);
    expect(Number(usage.costUsd)).toBeCloseTo(0.0009851, 7);

    const pricing = costs.pricing as { label: string } | undefined;
    expect(pricing?.label ?? "").toMatch(/reference pricing/i);
  });

  it("a bench report reaches the console with its verdict and its episode", async () => {
    await pool.query(
      `INSERT INTO foundry.bot_report
         (id, scene_id, slug, ran_at, verdict, missing_tools, stubbed_tools, shots,
          evidence_path, trajectory_id)
       VALUES ('e2e-br-1', 'flagtag', 'flagtag-arena', now(), 'pass', '[]', '[]', '[]',
               'packages/data/src/fixtures/trajectory-arena-sample.json',
               'traj-arena-flagtag-seed7')
       ON CONFLICT (id) DO NOTHING`,
    );

    const bench = await call(benchLoader as RouteHandler, get("/foundry/console/bench", sids.a));
    const reports = bench.reports as {
      slug: string;
      verdict: string | null;
      replayHref: string | null;
    }[];
    const arena = reports.find((r) => r.slug === "flagtag-arena");
    expect(arena).toBeTruthy();
    expect(arena!.verdict).toBe("pass");
    // The report links the episode it came from — the two are one record.
    expect(arena!.replayHref).toBe(
      "/foundry/console/trajectories/traj-arena-flagtag-seed7",
    );
  });

  it("a pledge by one visitor is a row the next one counts", async () => {
    const created = await call(
      exchangeAction as RouteHandler,
      post("/foundry/exchange", sids.author, {
        intent: "create",
        title: "A game that teaches its own controls",
        body: "The first thirty seconds should explain the mechanic without a wall of text.",
        source: "e2e",
      }),
    );
    expect(created.ok).toBe(true);

    const board = await call(exchangeLoader as RouteHandler, get("/foundry/exchange", sids.a));
    const requests = board.requests as { id: string; pledges: number }[];
    expect(requests.length).toBe(1);
    const requestId = requests[0].id;
    expect(requests[0].pledges).toBe(0);

    await call(
      exchangeAction as RouteHandler,
      post("/foundry/exchange", sids.a, { intent: "pledge", requestId }),
    );
    // One pledge per session: pledging twice is a no-op, not a second row.
    await call(
      exchangeAction as RouteHandler,
      post("/foundry/exchange", sids.a, { intent: "pledge", requestId }),
    );

    const asB = (
      (await call(exchangeLoader as RouteHandler, get("/foundry/exchange", sids.b)))
        .requests as { id: string; pledges: number; pledgedByMe: boolean }[]
    ).find((r) => r.id === requestId)!;
    expect(asB.pledges).toBe(1);
    expect(asB.pledgedByMe).toBe(false);
    // The count is the row count — there is no seeded addend left anywhere.
    expect(
      await count("SELECT count(*)::int AS n FROM foundry.pledge WHERE request_id = $1", [
        requestId,
      ]),
    ).toBe(1);

    // Server-fired telemetry carries the 4-hex badge, never the raw sid — so the
    // event is looked up by the badge, which is exactly the property the fix keeps
    // the raw session id out of the store.
    const submitted = await settledEvents(
      sidBadge(sids.a),
      "fd_pledge_submitted",
      1,
    );
    expect(submitted.length).toBe(1);

    await call(
      exchangeAction as RouteHandler,
      post("/foundry/exchange", sids.a, { intent: "withdraw", requestId }),
    );
    expect(
      await count("SELECT count(*)::int AS n FROM foundry.pledge WHERE request_id = $1", [
        requestId,
      ]),
    ).toBe(0);
  });

  it("every mutation above left an attributed action_log row, and reads left none", async () => {
    for (const sid of [sids.a, sids.author]) {
      expect(
        await count("SELECT count(*)::int AS n FROM foundry.action_log WHERE sid = $1", [sid]),
      ).toBeGreaterThan(0);
    }
    expect(
      await count("SELECT count(*)::int AS n FROM foundry.action_log WHERE sid = $1", [sids.b]),
    ).toBe(0);
  });

  it("the per-session write cap answers 429, not 500", async () => {
    let last: unknown = null;
    for (let i = 0; i < 12; i++) {
      last = await callRaw(
        exchangeAction as RouteHandler,
        post("/foundry/exchange", sids.cap, {
          intent: "create",
          title: `Write cap probe ${i}`,
          body: "One of a burst of writes from a single session, to prove the cap answers.",
          source: "e2e",
        }),
      );
    }
    expect(unwrap(last).ok).toBe(false);
    expect(String(unwrap(last).error)).toMatch(/too many writes/i);
    expect(statusOf(last)).toBe(429);
  });
});
