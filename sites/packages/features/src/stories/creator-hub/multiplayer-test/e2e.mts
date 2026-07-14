#!/usr/bin/env node
import { spawn, type ChildProcess, type SpawnOptions } from "node:child_process";
import crypto from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, writeFileSync } from "node:fs";
import http from "node:http";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { Tab, freePort, launchChromium } from "../../../../../../scripts/drive/cdp.mts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SITES = path.resolve(HERE, "../../../../../..");
const REPO = path.resolve(SITES, "..");

const CHROMIUM_VERSION = /-chromium-(\d+(?:\.\d+)*)$/;
function resolveChromiumBin(): string {
  if (process.env.CHROMIUM_BIN) return process.env.CHROMIUM_BIN;
  const best = readdirSync("/nix/store")
    .map((d) => ({ bin: `/nix/store/${d}/bin/chromium`, m: d.match(CHROMIUM_VERSION) }))
    .filter((c) => c.m && existsSync(c.bin))
    .map((c) => ({ bin: c.bin, v: c.m![1]!.split(".").map(Number) }))
    .sort((a, b) => {
      for (let i = 0; i < Math.max(a.v.length, b.v.length); i++) {
        const d = (a.v[i] ?? 0) - (b.v[i] ?? 0);
        if (d) return d;
      }
      return 0;
    })
    .at(-1);
  if (!best) throw new InfraError("no chromium in /nix/store \u{2014} set CHROMIUM_BIN");
  return best.bin;
}

class InfraError extends Error {}
class AssertError extends Error {}

type ArgValue = string | boolean;
const args: Record<string, ArgValue> = {};
{
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--spawn-sidecar" || a === "--keep" || a === "--unpaired-only") {
      args[a.slice(2)] = true;
    } else if (a.startsWith("--")) {
      args[a.slice(2)] = argv[++i]!;
    }
  }
}
const strArg = (key: string): string | undefined => {
  const v = args[key];
  return typeof v === "string" ? v : undefined;
};
const truthy = (key: string): boolean => !!args[key];

const SIDECAR_PORT = Number(strArg("port") ?? process.env.MP_TESTD_PORT ?? 5717);
const BASE = strArg("base") ?? "http://127.0.0.1:5197";
const SHOTS = strArg("shots-dir") ?? mkdtempSync(path.join(os.tmpdir(), "mp-panel-e2e-"));
mkdirSync(SHOTS, { recursive: true });
const log = (m: string): void => console.log(`[mp-e2e] ${m}`);
const ok = (m: string): void => console.log(`[mp-e2e] PASS ${m}`);

type HttpResult = { status: number | undefined; body: string };

function httpReq(url: string, opts: { method?: string; headers?: Record<string, string>; body?: string } = {}): Promise<HttpResult> {
  const { method = "GET", headers = {}, body } = opts;
  return new Promise((resolve, reject) => {
    const req = http.request(url, { method, headers }, (res) => {
      let data = "";
      res.on("data", (c) => (data += c));
      res.on("end", () => resolve({ status: res.statusCode, body: data }));
    });
    req.on("error", reject);
    if (body) req.write(body);
    req.end();
  });
}

const children: { label: string; p: ChildProcess }[] = [];
function spawnChild(label: string, bin: string, argv: string[], opts?: SpawnOptions): ChildProcess {
  const p = spawn(bin, argv, { stdio: ["ignore", "pipe", "pipe"], detached: true, ...opts });
  p.stdout?.on("data", () => {});
  p.stderr?.on("data", () => {});
  children.push({ label, p });
  return p;
}
function cleanup(): void {
  for (const { p } of children) {
    try {
      process.kill(-p.pid!, "SIGTERM");
    } catch {
      try {
        p.kill();
      } catch {}
    }
  }
}
process.on("exit", cleanup);
process.on("SIGINT", () => process.exit(2));

function sidecarSpeaksCors(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const req = http.request(
      {
        host: "127.0.0.1",
        port,
        path: "/",
        method: "GET",
        headers: { Origin: new URL(BASE).origin },
      },
      (res) => {
        resolve(!!res.headers["access-control-allow-origin"]);
        res.resume();
      },
    );
    req.on("error", () => resolve(false));
    req.end();
  });
}

function startCorsShim(listenPort: number, upstreamPort: number): Promise<http.Server> {
  const cors = (origin?: string): Record<string, string> => ({
    "Access-Control-Allow-Origin": origin ?? "*",
    "Access-Control-Allow-Headers": "authorization, content-type",
    "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
    Vary: "Origin",
  });
  const server = http.createServer((req, res) => {
    const origin = typeof req.headers.origin === "string" ? req.headers.origin : undefined;
    if (req.method === "OPTIONS") {
      res.writeHead(204, cors(origin));
      res.end();
      return;
    }
    const up = http.request(
      {
        host: "127.0.0.1",
        port: upstreamPort,
        path: req.url,
        method: req.method,
        headers: { ...req.headers, host: `127.0.0.1:${upstreamPort}` },
      },
      (ur) => {
        res.writeHead(ur.statusCode ?? 502, { ...ur.headers, ...cors(origin) });
        ur.pipe(res);
      },
    );
    up.on("error", () => {
      res.writeHead(502, cors(origin));
      res.end('{"error":"E_SHIM_UPSTREAM"}');
    });
    req.pipe(up);
  });
  server.on("upgrade", (req, sock, head) => {
    const up = net.connect(upstreamPort, "127.0.0.1", () => {
      let raw = `${req.method} ${req.url} HTTP/1.1\r\n`;
      for (let i = 0; i < req.rawHeaders.length; i += 2) {
        const k = req.rawHeaders[i]!;
        const v = k.toLowerCase() === "host" ? `127.0.0.1:${upstreamPort}` : req.rawHeaders[i + 1];
        raw += `${k}: ${v}\r\n`;
      }
      up.write(raw + "\r\n");
      if (head?.length) up.write(head);
      sock.pipe(up);
      up.pipe(sock);
    });
    up.on("error", () => sock.destroy());
    sock.on("error", () => up.destroy());
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(listenPort, "127.0.0.1", () => resolve(server));
  });
}

async function waitHttp(url: string, timeoutMs: number, headers: Record<string, string> = {}): Promise<HttpResult> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    try {
      const r = await httpReq(url, { headers });
      if (r.status && r.status < 500) return r;
    } catch {}
    if (Date.now() > deadline) throw new InfraError(`timeout waiting for ${url}`);
    await new Promise((r) => setTimeout(r, 500));
  }
}

let tab: Tab | undefined;
async function waitSel(sel: string, timeoutMs: number, label?: string): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const found = await tab!.ev(`!!document.querySelector(${JSON.stringify(sel)})`);
    if (found) return;
    if (Date.now() > deadline) {
      throw new AssertError(`${label ?? sel}: not found within ${timeoutMs}ms`);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
}
const q = (sel: string, expr: string): Promise<any> =>
  tab!.ev(`(() => { const el = document.querySelector(${JSON.stringify(sel)}); return el ? (${expr}) : null; })()`);
const clickSel = (sel: string): Promise<any> => q(sel, "(el.click(), true)");
async function setInput(sel: string, value: string): Promise<void> {
  const done = await tab!.ev(`(() => {
    const el = document.querySelector(${JSON.stringify(sel)});
    if (!el) return false;
    const proto = el instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value").set.call(el, ${JSON.stringify(value)});
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
    return true;
  })()`);
  if (!done) throw new AssertError(`setInput: ${sel} not found`);
}
let shotN = 0;
async function shot(name: string): Promise<void> {
  const file = path.join(SHOTS, `${String(++shotN).padStart(2, "0")}-${name}.png`);
  writeFileSync(file, Buffer.from(await tab!.screenshotB64(), "base64"));
  log(`shot ${file}`);
}
function consoleErrors({ includeNetwork }: { includeNetwork: boolean }): string[] {
  const out: string[] = [];
  for (const e of tab!.drainEvents()) {
    if (e.method === "Runtime.consoleAPICalled" && ["error", "assert"].includes(e.params?.type)) {
      out.push(`console.${e.params!.type}: ${JSON.stringify(e.params!.args?.[0]?.value ?? "").slice(0, 200)}`);
    } else if (e.method === "Runtime.exceptionThrown") {
      out.push(`exception: ${e.params?.exceptionDetails?.text ?? ""}`);
    } else if (
      includeNetwork &&
      e.method === "Log.entryAdded" &&
      e.params?.entry?.level === "error"
    ) {
      out.push(`log[${e.params.entry.source}]: ${e.params.entry.text?.slice(0, 200)}`);
    }
  }
  return out;
}
function assertNoErrors(label: string, { includeNetwork = false }: { includeNetwork?: boolean } = {}): void {
  const errs = consoleErrors({ includeNetwork });
  if (errs.length) {
    throw new AssertError(`${label}: expected zero console errors, got:\n  ${errs.join("\n  ")}`);
  }
  ok(`${label}: zero console errors`);
}

async function main(): Promise<void> {
  let token = strArg("token") ?? process.env.MP_TESTD_TOKEN ?? "";
  if (!token && !truthy("spawn-sidecar")) {
    throw new InfraError("--token <t> required (or --spawn-sidecar)");
  }

  let haveServer = false;
  try {
    const r = await httpReq(`${BASE}/create/multiplayer-test`);
    haveServer = r.status === 200;
  } catch {}
  if (!haveServer) {
    if (!existsSync(path.join(SITES, "build/server/index.js")) || truthy("rebuild")) {
      log("building sites (one-time)\u{2026}");
      const build = spawnChild("build", "npm", ["run", "build"], { cwd: SITES });
      const rc = await new Promise((r) => build.on("exit", r));
      if (rc !== 0) throw new InfraError(`sites build failed (${rc})`);
    }
    const port = new URL(BASE).port || "80";
    spawnChild("server", "npm", ["start"], {
      cwd: SITES,
      env: { ...process.env, PORT: port },
    });
    await waitHttp(`${BASE}/create/multiplayer-test`, 60_000);
  }
  log(`sites server up at ${BASE}`);

  process.env.DCL_SHOT_CHROMIUM ??= resolveChromiumBin();
  const cdpPort = await freePort();
  const profileDir = mkdtempSync(path.join(os.tmpdir(), "mp-e2e-profile-"));
  const chromium = await launchChromium({ port: cdpPort, profileDir });
  children.push({ label: "chromium", p: chromium });
  tab = await Tab.open(cdpPort);
  await tab.cmd("Log.enable");
  await tab.setViewport(1440, 960);

  if (truthy("spawn-sidecar")) {
    const held = await httpReq(`http://127.0.0.1:${SIDECAR_PORT}/`).catch(() => null);
    if (held) throw new InfraError(`:${SIDECAR_PORT} already has a listener \u{2014} stop it or pass --port`);
  }
  await tab.navigate(`${BASE}/create/multiplayer-test`, 2_500);
  await waitSel('[data-mp-state="unpaired"]', 10_000, "unpaired state");
  const cmdText = await q("[data-mp-command] code", "el.textContent");
  if (!cmdText?.includes("mp-testd")) {
    throw new AssertError(`unpaired command block missing, got: ${cmdText}`);
  }
  await shot("unpaired");
  assertNoErrors("unpaired load (sidecar stopped)", { includeNetwork: true });

  if (truthy("unpaired-only")) return;

  let pairPort = SIDECAR_PORT;
  if (truthy("spawn-sidecar")) {
    token = crypto.randomBytes(16).toString("hex");
    const runsDir = mkdtempSync(path.join(os.tmpdir(), "mp-e2e-runs-"));
    const upstreamPort = SIDECAR_PORT + 2;
    spawnChild("mp-testd", "node", ["dist/index.js"], {
      cwd: path.join(REPO, "tools/mp-testd"),
      env: {
        ...process.env,
        MP_TESTD_PORT: String(upstreamPort),
        MP_TESTD_TOKEN: token,
        MP_RUNS_DIR: runsDir,
        MP_RUN_SH: path.join(HERE, "e2e-lane.sh"),
        MP_ANALYZER: `node ${path.join(HERE, "e2e-analyzer.mts")}`,
      },
    });
    await waitHttp(`http://127.0.0.1:${upstreamPort}/`, 15_000);
    if (await sidecarSpeaksCors(upstreamPort)) {
      log("mp-testd speaks CORS natively \u{2014} drop the shim from this harness");
      pairPort = upstreamPort;
    } else {
      await startCorsShim(SIDECAR_PORT, upstreamPort);
      log(`CORS shim :${SIDECAR_PORT} -> mp-testd :${upstreamPort} (interim until WS1 CORS lands)`);
    }
  } else {
    await waitHttp(`http://127.0.0.1:${SIDECAR_PORT}/`, 15_000);
    if (!(await sidecarSpeaksCors(SIDECAR_PORT))) {
      pairPort = SIDECAR_PORT + 1;
      await startCorsShim(pairPort, SIDECAR_PORT);
      log(`CORS shim :${pairPort} -> mp-testd :${SIDECAR_PORT} (interim until WS1 CORS lands)`);
    }
  }
  const auth = { Authorization: `Bearer ${token}` };
  const probe = await httpReq(`http://127.0.0.1:${pairPort}/runs`, { headers: auth });
  if (probe.status !== 200) {
    throw new InfraError(`sidecar auth probe failed (${probe.status}): ${probe.body}`);
  }
  log(`mp-testd up (panel pairs via :${pairPort})`);

  await tab.navigate(
    `${BASE}/create/multiplayer-test?mpd=${pairPort}#mpdtoken=${token}`,
    2_500,
  );
  await waitSel('[data-mp-state="idle"]', 20_000, "idle (paired) state");
  await shot("idle-paired");
  ok("paired: ws connected, panel idle");

  await clickSel('input[name="mp-lane"][value="engine"]');
  await setInput('input[name="mp-bots"]', "5");
  await waitSel('[data-mp-error="bots-cap"]', 5_000, "bots-cap refusal");
  const launchDisabled = await q('[data-mp-action="launch"]', "el.disabled");
  if (launchDisabled !== true) throw new AssertError("launch not disabled at N=5 on engine lane");
  const engineFixtures = await tab.ev(
    `[...document.querySelectorAll('select[name="mp-fixture"] option')].map(o => o.value)`,
  );
  if (engineFixtures.some((f: string) => ["fastlane", "cleantheclub", "flagtag", "skychaser"].includes(f))) {
    throw new AssertError(`game fixtures visible on engine lane: ${engineFixtures}`);
  }
  await shot("engine-gates");
  ok(`engine lane: N>3 refused, game fixtures hidden (options: ${engineFixtures.join(",")})`);
  await clickSel('input[name="mp-lane"][value="mixed"]');
  const stillRefused = await q('[data-mp-error="bots-cap"]', "!!el");
  if (stillRefused !== true) throw new AssertError("mixed lane accepted N=5");
  ok("mixed lane: N>3 refused");
  await clickSel('input[name="mp-lane"][value="protocol"]');
  const protoFixtures = await tab.ev(
    `[...document.querySelectorAll('select[name="mp-fixture"] option')].map(o => o.value)`,
  );
  if (!protoFixtures.includes("fastlane") || !protoFixtures.includes("mp-sync")) {
    throw new AssertError(`protocol fixtures wrong: ${protoFixtures}`);
  }

  await setInput('input[name="mp-bots"]', "2");
  await setInput('input[name="mp-window"]', "6");
  await setInput('select[name="mp-fixture"]', "mp-sync");
  await setInput('select[name="mp-mode"]', "burst");
  await clickSel('[data-mp-action="launch"]');
  await waitSel('[data-mp-state="running"]', 20_000, "running state");
  await waitSel('[data-mp-bot="b1"]', 20_000, "bot b1 row");
  await waitSel('[data-mp-bot="b2"]', 10_000, "bot b2 row");
  await shot("running");
  ok("2-bot burst launched from the panel; live bot table populated over ws");

  await waitSel('[data-mp-state="reviewing"]', 120_000, "reviewing state");
  for (const s of ["starting", "running", "analyzing", "done"]) {
    await waitSel(`[data-mp-timeline-entry="${s}"]`, 5_000, `timeline entry ${s}`);
  }
  ok("observed starting \u{2192} running \u{2192} analyzing \u{2192} done over ws");

  await waitSel('[data-mp-metric="converge-ms"]', 30_000, "convergence metric");
  const converge = Number(await q('[data-mp-metric="converge-ms"]', "el.textContent"));
  if (!Number.isFinite(converge) || converge <= 0) {
    throw new AssertError(`convergence metric not numeric: ${converge}`);
  }
  ok(`numeric convergence metric: ${converge} ms (median)`);

  await waitSel("table[data-mp-divergence]", 10_000, "divergence table");
  const divHead = await q("table[data-mp-divergence] thead", "el.textContent");
  if (!/runner store/i.test(divHead ?? "")) {
    throw new AssertError(`divergence probe column not labeled 'runner store': ${divHead}`);
  }
  const divRows = await tab.ev(
    `document.querySelectorAll('table[data-mp-divergence] tbody tr').length`,
  );
  if (!divRows || divRows < 1) throw new AssertError("divergence table has no rows");
  ok(`divergence table rendered (${divRows} rows, probe column 'Runner store')`);

  const verdict = await q("[data-mp-verdict]", 'el.getAttribute("data-mp-verdict")');
  if (verdict !== "pass" && verdict !== "fail") {
    throw new AssertError(`verdict badge missing/pending: ${verdict}`);
  }
  ok(`verdict badge rendered: ${verdict.toUpperCase()}`);
  await shot("report");

  await clickSel('[data-mp-action="open-replay"]');
  await waitSel('[data-mp-tier="a"]', 5_000, "replay dialog");
  await shot("replay-dialog");
  await clickSel('[data-mp-tier="a"] input');
  await clickSel('[data-mp-action="replay"]');
  await waitSel("[data-mp-outcome-hash]", 90_000, "Tier A outcome hash");
  const hash = (await q("[data-mp-outcome-hash]", "el.textContent"))?.trim();
  if (!/^[0-9a-f]{8,64}$/i.test(hash ?? "")) {
    throw new AssertError(`Tier A outcome hash not hex: ${hash}`);
  }
  ok(`Tier A replay outcome hash: ${hash}`);
  await shot("replay-outcome");

  assertNoErrors("paired flow");
  console.log(`[mp-e2e] ALL PASS \u{2014} shots in ${SHOTS}`);
}

try {
  await main();
  process.exit(0);
} catch (e) {
  if (e instanceof AssertError) {
    console.error(`[mp-e2e] FAIL: ${e.message}`);
    try {
      if (tab) await shot("FAIL");
    } catch {}
    process.exit(1);
  }
  const msg = e instanceof Error ? e.message : String(e);
  console.error(`[mp-e2e] INFRA: ${msg}`);
  process.exit(2);
}
