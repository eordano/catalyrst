#!/usr/bin/env node
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { freePort, launchChromium, Tab, type CdpMessage } from "./cdp.mts";
import { ROUTES, type Route } from "./smoke-routes.mts";

const HERE = path.dirname(fileURLToPath(import.meta.url));

type Opts = {
  base: string;
  wait: number;
  json: string;
  routes: string[] | null;
};

function parseArgs(argv: string[]): Opts {
  const opts: Opts = {
    base: "https://catalyst.example.com",
    wait: 4000,
    json: path.join(HERE, "out", "smoke.json"),
    routes: null,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--base") opts.base = argv[++i]!;
    else if (a === "--wait") opts.wait = Number(argv[++i]);
    else if (a === "--json") opts.json = argv[++i]!;
    else if (a === "--routes") opts.routes = argv[++i]!.split(",");
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

async function main(): Promise<void> {
  const opts = parseArgs(process.argv.slice(2));
  const git = gitStamp();
  const routes: Route[] = opts.routes
    ? opts.routes.map(
        (p) => ROUTES.find((r) => r.path === p) ?? { path: p, auth: "out" as const },
      )
    : ROUTES;
  console.log(
    `[smoke] ${routes.length} routes vs ${opts.base} \u{B7} ${git.stamp}`,
  );

  const port = await freePort();
  const profile = fs.mkdtempSync("/tmp/smoke-");
  const chromium = await launchChromium({ port, profileDir: profile });
  const results: ResultRow[] = [];
  try {
    const tab = await Tab.open(port);
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
  } finally {
    const exited = new Promise((r) => chromium.once("exit", r));
    chromium.kill();
    await Promise.race([exited, new Promise((r) => setTimeout(r, 4000))]);
    try {
      fs.rmSync(profile, { recursive: true, force: true });
    } catch {
    }
  }

  const failed = results.filter((r) => r.failures.length);
  fs.mkdirSync(path.dirname(opts.json), { recursive: true });
  fs.writeFileSync(
    opts.json,
    JSON.stringify({ base: opts.base, git, results }, null, 2),
  );
  console.log(`[smoke] report: ${opts.json}`);
  if (failed.length) {
    console.error(`\n[smoke] ${failed.length} route-state(s) FAILED:`);
    for (const f of failed) {
      console.error(`  ${f.auth} ${f.path} (status ${f.status}):`);
      for (const i of f.failures) console.error(`    - [${i.kind}] ${i.text}`);
    }
    process.exit(1);
  }
  console.log(`[smoke] GREEN \u{2014} ${results.length} route-states clean`);
}

main().catch((err) => {
  console.error("[smoke]", err);
  process.exit(1);
});
