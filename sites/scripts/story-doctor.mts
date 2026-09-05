#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SITES = path.resolve(HERE, "..");

function env(name: string): string | undefined {
  try {
    return typeof process !== "undefined" ? process.env?.[name] : undefined;
  } catch {
    return undefined;
  }
}

type Opts = {
  sbBase: string;
  telemetryUrl: string;
  port: number;
};

function parseArgs(argv: string[]): Opts {
  const opts: Opts = {
    sbBase: env("SB_BASE") ?? "http://127.0.0.1:5006",
    telemetryUrl: env("TELEMETRY_URL") ?? "http://127.0.0.1:5150",
    port: 5173,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--sb-base") opts.sbBase = argv[++i]!;
    else if (a === "--telemetry-url") opts.telemetryUrl = argv[++i]!;
    else if (a === "--port") opts.port = Number(argv[++i]);
    else {
      console.error(
        "usage: story-doctor.mts [--sb-base URL] [--telemetry-url URL] [--port N]",
      );
      process.exit(2);
    }
  }
  if (!Number.isInteger(opts.port) || opts.port <= 0) {
    console.error("story-doctor: --port must be a positive integer");
    process.exit(2);
  }
  return opts;
}

function pass(name: string, detail?: string): void {
  console.log(`PASS ${name}${detail ? ` -- ${detail}` : ""}`);
}

function fail(name: string, detail: string, hint: string): void {
  process.exitCode = 1;
  console.log(`FAIL ${name} -- ${detail}; hint: ${hint}`);
}

async function checkHttp200(
  name: string,
  url: string,
  hint: string,
): Promise<void> {
  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(5000) });
    if (res.status === 200) pass(name);
    else fail(name, `GET ${url} -> ${res.status}`, hint);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    fail(name, `GET ${url} failed (${msg})`, hint);
  }
}

function firstLine(out: string, fallback: string): string {
  return (
    out
      .split("\n")
      .map((l) => l.trim())
      .find((l) => l.length > 0) ?? fallback
  );
}

function checkNpmScript(name: string, script: string, hint: string): void {
  const r = spawnSync("npm", ["run", "--silent", script], {
    cwd: SITES,
    encoding: "utf8",
    timeout: 120_000,
  });
  if (r.status === 0) pass(name);
  else {
    const out = `${r.stdout ?? ""}\n${r.stderr ?? ""}`;
    fail(name, firstLine(out, `exit ${r.status}`), hint);
  }
}

function dirtySpecFiles(): string[] {
  const r = spawnSync(
    "git",
    ["status", "--porcelain", "--", "packages/features/src/stories"],
    { cwd: SITES, encoding: "utf8" },
  );
  return (r.stdout ?? "")
    .split("\n")
    .filter((l) => l.trimEnd().endsWith("spec.stories.tsx"))
    .map((l) => l.slice(3).trim());
}

function checkSpecStoriesClean(): void {
  const name = "spec-stories-clean";
  const hint =
    "spec.stories.tsx were stale and have been regenerated -- review the diff and let the landing agent commit; edit story.md, never spec.stories.tsx";
  const before = dirtySpecFiles();
  const gen = spawnSync("node", ["scripts/gen-spec-stories.mts"], {
    cwd: SITES,
    encoding: "utf8",
    timeout: 120_000,
  });
  if (gen.status !== 0) {
    const out = `${gen.stdout ?? ""}\n${gen.stderr ?? ""}`;
    fail(name, `gen-spec-stories failed: ${firstLine(out, `exit ${gen.status}`)}`, hint);
    return;
  }
  if (before.length > 0) {
    console.log(
      `note: pre-existing dirty spec files (not caused by doctor): ${before.join(", ")}`,
    );
  }
  const beforeSet = new Set(before);
  const fresh = dirtySpecFiles().filter((f) => !beforeSet.has(f));
  if (fresh.length === 0) pass(name);
  else fail(name, `stale specs regenerated: ${fresh.join(", ")}`, hint);
}

async function checkTelemetrySql(telemetryUrl: string): Promise<void> {
  const name = "telemetry-sql";
  const token = env("TELEMETRY_ADMIN_TOKEN");
  if (!token) {
    fail(
      name,
      "TELEMETRY_ADMIN_TOKEN unset",
      "export TELEMETRY_ADMIN_TOKEN (value of CATALYRST_TELEMETRY_ADMIN_TOKEN in the telemetry service env) and TELEMETRY_URL",
    );
    return;
  }
  const base = telemetryUrl.replace(/\/+$/, "");
  try {
    const res = await fetch(`${base}/dash/sql`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ sql: "SELECT 1" }),
      signal: AbortSignal.timeout(5000),
    });
    if (res.status === 200) pass(name);
    else if (res.status === 403)
      fail(name, "token rejected (403)", "TELEMETRY_ADMIN_TOKEN is stale/wrong");
    else
      fail(
        name,
        `unreachable (status ${res.status})`,
        "is catalyrst-telemetry up on :5150?",
      );
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    fail(name, `unreachable (${msg})`, "is catalyrst-telemetry up on :5150?");
  }
}

function checkPortSquat(port: number): void {
  const name = "port-squat";
  const hint = "kill the squatter or run dev on another port";
  const r = spawnSync("ss", ["-tlnp"], { encoding: "utf8", timeout: 10_000 });
  if (r.status !== 0) {
    fail(name, `ss -tlnp failed (exit ${r.status})`, "is iproute2 on PATH?");
    return;
  }
  const needle = `:${port} `;
  const line = (r.stdout ?? "")
    .split("\n")
    .find((l) => l.includes("LISTEN") && l.includes(needle));
  if (!line) {
    pass(name, `port ${port} free`);
    return;
  }
  const users = line.match(/users:\(\(.*\)\)/)?.[0];
  fail(
    name,
    `listener on port ${port} (${users ?? "owned by another user"})`,
    hint,
  );
}

async function main(): Promise<void> {
  const opts = parseArgs(process.argv.slice(2));
  console.log(
    `story-doctor: sb=${opts.sbBase} telemetry=${opts.telemetryUrl} dev-port=${opts.port}`,
  );
  console.log(
    "note: the spec-stories-clean check MAY WRITE regenerated spec.stories.tsx files (deterministic generator output); all other checks are read-only and git status is the only git subcommand used",
  );

  await checkHttp200(
    "storybook",
    `${opts.sbBase}/`,
    "start it: (cd ../ui3 && npm run storybook) and wait for :5006",
  );

  const storyAbs = path.join(
    SITES,
    "packages/features/src/stories/client/open-screen/OpenScreen.stories.tsx",
  );
  await checkHttp200(
    "sb-transform",
    `${opts.sbBase}/@fs${storyAbs}`,
    "storybook cannot serve sites stories -- check ui3/.storybook/main.ts stories globs + viteFinal fs.allow, then restart storybook",
  );

  checkNpmScript("gen:story-ids:check", "gen:story-ids:check", "npm run gen:story-ids");
  checkNpmScript("gen:schemas:check", "gen:schemas:check", "npm run gen:schemas");
  checkNpmScript("telemetry:check", "telemetry:check", "npm run telemetry:catalog");
  checkSpecStoriesClean();
  await checkTelemetrySql(opts.telemetryUrl);
  checkPortSquat(opts.port);
}

main().catch((err) => {
  console.error(
    `story-doctor error: ${err instanceof Error ? err.message : String(err)}`,
  );
  process.exit(1);
});
