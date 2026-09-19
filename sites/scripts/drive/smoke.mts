#!/usr/bin/env node
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { freePort, launchChromium, Tab, type CdpMessage } from "./cdp.mts";
import { HUD_ALLOW, ROUTES, hudPanels, type Route } from "./smoke-routes.mts";

const HERE = path.dirname(fileURLToPath(import.meta.url));

type Opts = {
  base: string;
  wait: number;
  json: string;
  routes: string[] | null;
  hud: boolean;
  hudOnly: boolean;
  cdpPort: number | null;
};

function parseArgs(argv: string[]): Opts {
  const opts: Opts = {
    base: "https://catalyst.example.com",
    wait: 4000,
    json: path.join(HERE, "out", "smoke.json"),
    routes: null,
    hud: false,
    hudOnly: false,
    cdpPort: process.env.SMOKE_CDP_PORT ? Number(process.env.SMOKE_CDP_PORT) : null,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--base") opts.base = argv[++i]!;
    else if (a === "--wait") opts.wait = Number(argv[++i]);
    else if (a === "--json") opts.json = argv[++i]!;
    else if (a === "--routes") opts.routes = argv[++i]!.split(",");
    else if (a === "--hud") opts.hud = true;
    else if (a === "--hud-only") opts.hud = opts.hudOnly = true;
    else if (a === "--cdp-port") opts.cdpPort = Number(argv[++i]);
  }
  return opts;
}

function gitStamp(): { commit: string; dirty: boolean; stamp: string } {
  const run = (cmd: string): string => execSync(cmd, { encoding: "utf8", cwd: HERE }).trim();
  const commit = run("git rev-parse --short HEAD");
  const dirty = run("git status --porcelain -- . | head -5").length > 0
    ? true
    : run("git -C ../../../.. status --porcelain -- catalyrst/sites catalyrst/ui3 | head -5").length > 0;
  return { commit, dirty, stamp: `${commit}${dirty ? "+dirty" : ""}` };
}

type Issue = { kind: string; text: string };

function collectIssues(events: CdpMessage[], docUrl: string, allowConsole: RegExp[] = []): { issues: Issue[]; status: number | null } {
  const issues: Issue[] = [];
  let status: number | null = null;
  for (const e of events) {
    const p = e.params ?? {};
    if (e.method === "Network.responseReceived") {
      if (p.type === "Document" && p.response?.url?.split("#")[0] === docUrl) {
        status = p.response.status;
      }
      continue;
    }
    let text: string | null = null;
    let kind: string | null = null;
    if (e.method === "Runtime.consoleAPICalled" && p.type === "error") {
      text = (p.args ?? [])
        .map((a: { value?: unknown; description?: string }) => a.value ?? a.description ?? "")
        .join(" ");
      kind = "console.error";
    } else if (e.method === "Runtime.exceptionThrown") {
      const d = p.exceptionDetails ?? {};
      text = d.exception?.description ?? d.text ?? "unknown exception";
      kind = "exception";
    } else if (e.method === "Log.entryAdded" && p.entry?.level === "error") {
      text = `${p.entry.source}: ${p.entry.text}${p.entry.url ? ` (${p.entry.url})` : ""}`;
      kind = `log.${p.entry.source}`;
    }
    if (!text) continue;
    if (allowConsole.some((re) => re.test(text!))) continue;
    issues.push({ kind: kind!, text: text.slice(0, 300) });
  }
  return { issues, status };
}

const PAGE_CHECKS = `(() => {
  const broken = [...document.images]
    .filter((i) => i.src && (!i.complete || i.naturalWidth === 0))
    .map((i) => i.currentSrc || i.src)
    .slice(0, 10);
  let fontOk = true;
  try { fontOk = document.fonts.check('16px Inter'); } catch {}
  return JSON.stringify({
    broken,
    fontOk,
    title: document.title,
    bodyChars: (document.body?.innerText ?? '').length,
  });
})()`;

const RETRY_DELAY_MS = 10_000;

type PageChecks = {
  broken: string[];
  fontOk: boolean;
  title: string;
  bodyChars: number;
};

type VisitResult = {
  status: number | null;
  failures: Issue[];
  title: string;
  transient?: Issue[];
};

async function visitOnce(tab: Tab, url: string, waitMs: number, allowConsole: RegExp[]): Promise<VisitResult> {
  tab.drainEvents();
  await tab.cmd("Page.navigate", { url });
  await new Promise((r) => setTimeout(r, waitMs));
  const page: PageChecks = JSON.parse(await tab.ev(PAGE_CHECKS));
  const { issues, status } = collectIssues(tab.drainEvents(), url, allowConsole);
  const failures: Issue[] = [...issues];
  if (status !== null && status >= 400) {
    failures.unshift({ kind: "http", text: `document status ${status}` });
  }
  if (page.broken.length) {
    failures.push({
      kind: "broken-images",
      text: `${page.broken.length} broken: ${page.broken.join(", ").slice(0, 250)}`,
    });
  }
  if (!page.fontOk) {
    failures.push({ kind: "fonts", text: "brand font (Inter) not loaded" });
  }
  if (page.bodyChars < 50) {
    failures.push({ kind: "empty-page", text: `body has ${page.bodyChars} chars` });
  }
  return { status, failures, title: page.title };
}

async function visit(tab: Tab, url: string, waitMs: number, allowConsole: RegExp[]): Promise<VisitResult> {
  let first: VisitResult;
  try {
    first = await visitOnce(tab, url, waitMs, allowConsole);
  } catch (err) {
    // A transport fault (tab.ev hitting its 30s "ws recv timeout" while the box
    // is loaded) must not escape visit() and abort the whole sweep -- it gets
    // the same single retry any other failure gets. A retry is safe here:
    // cmd() matches strictly on message id so a late response cannot be
    // mistaken for this one, and visitOnce() opens with drainEvents().
    const message = err instanceof Error ? err.message : String(err);
    console.log(`[smoke]   ${url}: ${message}, retrying in ${RETRY_DELAY_MS / 1000}s...`);
    await new Promise((r) => setTimeout(r, RETRY_DELAY_MS));
    const retried = await visitOnce(tab, url, waitMs, allowConsole);
    return { ...retried, transient: [{ kind: "transport", text: message }] };
  }
  if (!first.failures.length) return first;
  console.log(`[smoke]   ${url}: ${first.failures.length} failure(s), retrying in ${RETRY_DELAY_MS / 1000}s...`);
  await new Promise((r) => setTimeout(r, RETRY_DELAY_MS));
  const second = await visitOnce(tab, url, waitMs, allowConsole);
  if (!second.failures.length) {
    return { ...second, transient: first.failures };
  }
  return second;
}

type ResultRow = { path: string; auth: string } & VisitResult;

type HudReport = { mode: string; panels: number; walked: number; skipped: string | null; failures: Issue[] };

// The repo's skip contract (test/e2e/require-dep.ts; scripts/no-silent-skips.sh
// holds this literal to the Rust and shell ones): a check that could not run
// fails, unless the opt-out is set, and then it says so with this marker.
const OPT_OUT = "ALLOW_SKIPPED_INTEGRATION";
const skipsAllowed = !["", "0", "false"].includes((process.env[OPT_OUT] ?? "").toLowerCase());
function marker(name: string, requirement: string, detail: string): string {
  return `SKIPPED ${name}: ${requirement} unavailable (${detail}); ${OPT_OUT} is set`;
}

// Two lobby screens, and a fresh profile meets both: the guest form (terms box
// gates the button), then -- once the world has loaded -- LobbyHome, whose
// "Enter the world" is what mounts the HUD. A profile that already holds an
// identity starts at LobbyHome.
const LOBBY_READY = `!!document.querySelector(".lobbynew__jump-label, .lh__welcome")`;
const LOBBY_ENTER = `(() => {
  const box = document.querySelector(".lobbynew__checks input");
  if (box && !box.checked) box.click();
  const jump = document.querySelector("button.lobbynew__jump") ??
    [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === "Enter the world");
  if (!jump || jump.disabled) return false;
  jump.click();
  return true;
})()`;
const ENGINE_DOWN = `(() => {
  if (!("gpu" in navigator)) return "no navigator.gpu";
  const t = (document.body.innerText || "").toLowerCase();
  const hit = ["world crashed", "initialize the graphics", "webgpu unavailable", "engine didn"].find((s) => t.includes(s));
  return hit ? 'page says "' + hit + '"' : "";
})()`;

// Lobby -> continue as guest -> every HUD panel by hash. The HUD mounts only once
// the engine reports world-ready, so a browser that cannot bring WebGPU up (the
// one launched here runs --disable-gpu) stops at the lobby with no panel
// walked. To walk them, attach to a GPU chromium with --cdp-port
// (rig/lib/chromium-launch.sh starts one).
async function hudLane(tab: Tab, base: string): Promise<HudReport> {
  const failures: Issue[] = [];
  let evErr = "";
  const note = (step: string): void => {
    for (const i of collectIssues(tab.drainEvents(), "", HUD_ALLOW).issues) {
      failures.push({ kind: i.kind, text: `${step}: ${i.text}` });
    }
  };
  const until = async (step: string, expr: string, timeoutMs: number): Promise<boolean> => {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      note(step);
      if (await tab.ev(expr).catch((err) => { evErr = String(err instanceof Error ? err.message : err).slice(0, 200); return false; })) return true;
      if (Date.now() > deadline) return false;
      await new Promise((r) => setTimeout(r, 500));
    }
  };

  let uidev = false;
  try {
    await fetch(`http://localhost:${process.env.DCL_UIDEV_PORT || 5174}/@vite/client`);
    uidev = true;
  } catch {
  }
  const panels = hudPanels();
  const report: HudReport = { mode: uidev ? "uidev" : "committed", panels: panels.length, walked: 0, skipped: null, failures };
  console.log(`[smoke] hud: ${panels.length} panels, ${uidev ? "?uidev=1 (overlay dev server up)" : "committed overlay"}`);

  tab.drainEvents();
  await tab.cmd("Page.navigate", { url: `${base}/_play/?uidev=${uidev ? 1 : 0}&realm=${base}&position=0,0` });
  if (!(await until("hud:lobby", LOBBY_READY, 60_000))) {
    failures.push({ kind: "hud", text: "hud:lobby: the lobby never rendered" });
    return report;
  }
  if (!(await until("hud:enter", LOBBY_ENTER, 8_000))) {
    failures.push({ kind: "hud", text: "hud:enter: no enabled way in (ticking the terms box never enabled the guest button)" });
    return report;
  }
  console.log("[smoke] hud: through the lobby, waiting for the engine");
  const mounted = `!!document.querySelector(".ui3-overlay__sidebar")`;
  const enterWorld = `(() => {
    const b = [...document.querySelectorAll("button")].find((x) => x.textContent.trim() === "Enter the world" && !x.disabled);
    if (b && !window.__smokeEnteredWorld) { window.__smokeEnteredWorld = true; b.click(); }
    return false;
  })()`;
  await until("hud:jump-in", `${mounted} || ${enterWorld} || ${ENGINE_DOWN}`, 90_000);
  if (!(await tab.ev(mounted).catch(() => false))) {
    const down = await tab.ev(ENGINE_DOWN).catch((err) => `page stopped answering: ${err instanceof Error ? err.message : err}`.slice(0, 240));
    if (evErr) console.error(`[smoke] hud: last evaluate error while waiting: ${evErr}`);
    if (down && skipsAllowed) {
      report.skipped = down;
      console.error(marker("smoke-hud", "a browser with WebGPU", `${down}; lobby and way in checked, no panel walked`));
    } else if (down) {
      failures.push({
        kind: "hud",
        text: `hud:jump-in: no panel walked, the engine did not come up in this browser (${down}). Attach one with WebGPU (--cdp-port), or set ${OPT_OUT}=1 on a machine that has none`,
      });
    } else {
      failures.push({ kind: "hud", text: "hud:jump-in: engine booted but the HUD sidebar never mounted" });
    }
    return report;
  }
  await tab.ev(`[...document.querySelectorAll("button")].find((b) => b.textContent.trim() === "Dismiss")?.click()`);
  for (const id of panels) {
    await tab.ev(`location.hash = "#/${id}"`);
    await new Promise((r) => setTimeout(r, 700));
    note(`panel:#/${id}`);
    const hash = await tab.ev("location.hash");
    if (hash !== `#/${id}`) {
      failures.push({ kind: "hud", text: `panel:#/${id}: router bounced to ${hash || "(empty)"}` });
    }
    report.walked++;
  }
  return report;
}

async function main(): Promise<void> {
  const opts = parseArgs(process.argv.slice(2));
  const git = gitStamp();
  const routes: Route[] = opts.hudOnly
    ? []
    : opts.routes
    ? opts.routes.map(
        (p) => ROUTES.find((r) => r.path === p) ?? { path: p, auth: "out" as const },
      )
    : ROUTES;
  console.log(
    `[smoke] ${routes.length} routes vs ${opts.base} \u{B7} ${git.stamp}`,
  );

  const port = opts.cdpPort ?? (await freePort());
  const profile = fs.mkdtempSync("/tmp/smoke-");
  const chromium = opts.cdpPort ? null : await launchChromium({ port, profileDir: profile });
  const results: ResultRow[] = [];
  let hud: HudReport | null = null;
  let attached: Tab | null = null;
  try {
    const tab = await Tab.open(port);
    if (!chromium) attached = tab;
    await tab.cmd("Log.enable");
    await tab.cmd("Network.enable");

    for (const r of routes) {
      const url = opts.base + r.path;
      const res = await visit(tab, url, opts.wait, r.allowConsole ?? []);
      results.push({ path: r.path, auth: "out", ...res });
      console.log(
        `[smoke] out ${r.path}: ${res.failures.length ? "FAIL " + res.failures.length : res.transient ? "ok (transient warmup cleared)" : "ok"}`,
      );
    }

    const authed = routes.filter((r) => r.auth === "both");
    if (authed.length) {
      await tab.cmd("Page.navigate", { url: opts.base + "/" });
      await new Promise((r) => setTimeout(r, 3000));
      const signed = await tab.ev(
        "window.__DCL_DEV__ ? window.__DCL_DEV__.signInBurner().then(id => id.signer) : null",
        { awaitPromise: true, timeoutMs: 20_000 },
      );
      if (!signed) {
        console.log(
          "[smoke] burner unavailable (no __DCL_DEV__ \u{2014} not a dev server?): skipping authed pass",
        );
      } else {
        console.log(`[smoke] authed pass as ${signed}`);
        for (const r of authed) {
          const url = opts.base + r.path;
          const res = await visit(tab, url, opts.wait, r.allowConsole ?? []);
          results.push({ path: r.path, auth: "in", ...res });
          console.log(
            `[smoke] in  ${r.path}: ${res.failures.length ? "FAIL " + res.failures.length : res.transient ? "ok (transient warmup cleared)" : "ok"}`,
          );
        }
      }
    }

    if (opts.hud) {
      hud = await hudLane(tab, opts.base);
      console.log(
        `[smoke] hud: ${hud.failures.length ? "FAIL " + hud.failures.length : hud.skipped ? "skipped" : `ok (${hud.walked} panels)`}`,
      );
    }
  } finally {
    // Someone else's browser: leave it running, take only our tab back.
    await attached?.cmd("Page.close", {}, 5_000).catch(() => {});
    if (chromium) {
      const exited = new Promise((r) => chromium.once("exit", r));
      chromium.kill();
      await Promise.race([exited, new Promise((r) => setTimeout(r, 4000))]);
    }
    try {
      fs.rmSync(profile, { recursive: true, force: true });
    } catch {
    }
  }

  const failed = results.filter((r) => r.failures.length);
  fs.mkdirSync(path.dirname(opts.json), { recursive: true });
  fs.writeFileSync(
    opts.json,
    JSON.stringify({ base: opts.base, git, results, ...(hud ? { hud } : {}) }, null, 2),
  );
  console.log(`[smoke] report: ${opts.json}`);
  if (failed.length || hud?.failures.length) {
    console.error(`\n[smoke] FAILED: ${failed.length} route-state(s)${hud?.failures.length ? `, ${hud.failures.length} hud` : ""}`);
    for (const f of failed) {
      console.error(`  ${f.auth} ${f.path} (status ${f.status}):`);
      for (const i of f.failures) console.error(`    - [${i.kind}] ${i.text}`);
    }
    for (const i of hud?.failures ?? []) console.error(`  hud - [${i.kind}] ${i.text}`);
    process.exit(1);
  }
  console.log(`[smoke] GREEN \u{2014} ${results.length} route-states clean${hud ? (hud.skipped ? ", hud skipped" : `, ${hud.walked} hud panels`) : ""}`);
}

main().catch((err) => {
  console.error("[smoke]", err);
  process.exit(1);
});
