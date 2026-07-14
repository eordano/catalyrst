import { execFileSync, spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
  existsSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { Pool } from "pg";

import { seedPlaces } from "./seed";
import { bootstrapTelemetry } from "./telemetry-seed";
import { seedCreateEntryPreview } from "./create-telemetry-seed";

const SCHEMA = fileURLToPath(new URL("./schema.sql", import.meta.url));
const STATE_FILE = join(tmpdir(), "sites-e2e-pg-state.json");

type PgState =
  | { kind: "provided"; url: string; adminUrl: string; dbname: string }
  | { kind: "pg_tmp"; url: string }
  | { kind: "cluster"; url: string; datadir: string; sockdir: string };

function onPath(bin: string): boolean {
  const r = spawnSync(bin, ["--version"], { stdio: "ignore" });
  return r.status === 0 || r.error === undefined;
}

const UNROOT_C = `#define _GNU_SOURCE
#include <sys/types.h>
#include <sys/stat.h>
#include <unistd.h>
#include <dlfcn.h>
static const uid_t U=1000; static const gid_t G=100;
uid_t geteuid(void){return U;} uid_t getuid(void){return U;}
gid_t getegid(void){return G;} gid_t getgid(void){return G;}
int getresuid(uid_t*r,uid_t*e,uid_t*s){if(r)*r=U;if(e)*e=U;if(s)*s=U;return 0;}
int getresgid(gid_t*r,gid_t*e,gid_t*s){if(r)*r=G;if(e)*e=G;if(s)*s=G;return 0;}
#define FIX(b) do{(b)->st_uid=U;(b)->st_gid=G;}while(0)
int stat(const char*p,struct stat*b){static int(*f)(const char*,struct stat*)=0;if(!f)f=dlsym(RTLD_NEXT,"stat");int r=f(p,b);if(!r)FIX(b);return r;}
int lstat(const char*p,struct stat*b){static int(*f)(const char*,struct stat*)=0;if(!f)f=dlsym(RTLD_NEXT,"lstat");int r=f(p,b);if(!r)FIX(b);return r;}
int fstat(int d,struct stat*b){static int(*f)(int,struct stat*)=0;if(!f)f=dlsym(RTLD_NEXT,"fstat");int r=f(d,b);if(!r)FIX(b);return r;}
int fstatat(int d,const char*p,struct stat*b,int fl){static int(*f)(int,const char*,struct stat*,int)=0;if(!f)f=dlsym(RTLD_NEXT,"fstatat");int r=f(d,p,b,fl);if(!r)FIX(b);return r;}
`;

let _pgEnv: NodeJS.ProcessEnv | undefined;
function pgSpawnEnv(): NodeJS.ProcessEnv {
  if (_pgEnv) return _pgEnv;
  _pgEnv = buildPgSpawnEnv();
  return _pgEnv;
}
function buildPgSpawnEnv(): NodeJS.ProcessEnv {
  const isRoot = typeof process.getuid === "function" && process.getuid() === 0;
  if (!isRoot) return process.env;
  const cc = ["cc", "gcc"].find(
    (c) => spawnSync(c, ["--version"], { stdio: "ignore" }).status === 0,
  );
  if (!cc) {
    console.warn(
      "[e2e] running as root but no C compiler found — cannot build the Postgres " +
        "uid shim; the DB suite will skip. Install cc/gcc or run unprivileged.",
    );
    return process.env;
  }
  try {
    const dir = mkdtempSync(join(tmpdir(), "sites-e2e-unroot-"));
    const src = join(dir, "unroot.c");
    const so = join(dir, "unroot.so");
    writeFileSync(src, UNROOT_C, "utf8");
    const r = spawnSync(
      cc,
      ["-shared", "-fPIC", "-O2", "-o", so, src, "-ldl"],
      { stdio: "ignore" },
    );
    if (r.status !== 0) throw new Error("compile failed");
    const LD_PRELOAD = [so, process.env.LD_PRELOAD].filter(Boolean).join(" ");
    console.log(
      "[e2e] root detected — Postgres will run under an LD_PRELOAD uid shim " +
        "(reports uid 1000; no privilege drop).",
    );
    return { ...process.env, LD_PRELOAD };
  } catch (e) {
    console.warn(
      "[e2e] failed to build the Postgres uid shim — DB suite will skip:",
      (e as Error).message,
    );
    return process.env;
  }
}

function tryPgTmp(): string | null {
  const env = pgSpawnEnv();
  const probe = spawnSync("pg_tmp", ["-h"], { encoding: "utf8", env });
  if (probe.error) return null;
  const r = spawnSync("pg_tmp", ["-t", "-w", "60"], { encoding: "utf8", env });
  if (r.status !== 0 || !r.stdout) return null;
  const url = r.stdout.trim().split("\n").pop()!.trim();
  return url || null;
}

function tryCluster(): Extract<PgState, { kind: "cluster" }> | null {
  if (!onPath("initdb") || !onPath("pg_ctl")) return null;
  try {
    return tryClusterInner();
  } catch {
    // A present-but-broken postgres (e.g. system initdb missing its share/
    // files outside `nix develop`) must degrade to the no-pg skip path,
    // not hard-fail the whole e2e suite.
    return null;
  }
}

function tryClusterInner(): Extract<PgState, { kind: "cluster" }> | null {
  const base = mkdtempSync(join(tmpdir(), "sites-e2e-pg-"));
  const datadir = join(base, "data");
  const sockdir = join(base, "sock");
  execFileSync("mkdir", ["-p", sockdir]);
  const env = pgSpawnEnv();
  execFileSync("initdb", [
    "-D",
    datadir,
    "-A",
    "trust",
    "-U",
    "postgres",
    "--no-sync",
  ], { stdio: "ignore", env });
  execFileSync(
    "pg_ctl",
    [
      "-D",
      datadir,
      "-o",
      `-k ${sockdir} -c listen_addresses='' -c fsync=off`,
      "-w",
      "-t",
      "60",
      "start",
    ],
    { stdio: "ignore", env },
  );
  const url = `postgresql://postgres@/postgres?host=${encodeURIComponent(sockdir)}`;
  return { kind: "cluster", url, datadir, sockdir };
}

async function applySchemaAndSeed(url: string): Promise<void> {
  const pool = new Pool({ connectionString: url, max: 2 });
  try {
    await pool.query(readFileSync(SCHEMA, "utf8"));
    const n = await seedPlaces(pool);
    // eslint-disable-next-line no-console
    console.log(`[e2e] seeded ${n} places into temp postgres`);

    const ev = await bootstrapTelemetry(pool);
    // eslint-disable-next-line no-console
    console.log(`[e2e] seeded ${ev} telemetry events into temp postgres`);

    const cev = await seedCreateEntryPreview(pool);
    // eslint-disable-next-line no-console
    console.log(`[e2e] seeded ${cev} create telemetry events into temp postgres`);
  } finally {
    await pool.end();
  }
}

// schema.sql is not idempotent, so the suite gets its own database on the
// provided server rather than assuming that server is fresh. Same shape as
// catalyrst's ScratchDb, and non-destructive: nothing pre-existing is dropped.
async function provisionScratchDb(
  adminUrl: string,
): Promise<Extract<PgState, { kind: "provided" }>> {
  const dbname = `sites_e2e_${randomBytes(6).toString("hex")}`;
  const admin = new Pool({ connectionString: adminUrl, max: 1 });
  try {
    await admin.query(`CREATE DATABASE "${dbname}"`);
  } finally {
    await admin.end();
  }
  const u = new URL(adminUrl);
  u.pathname = `/${dbname}`;
  return { kind: "provided", url: u.toString(), adminUrl, dbname };
}

export async function setup(): Promise<void> {
  if (existsSync(STATE_FILE)) rmSync(STATE_FILE, { force: true });

  let state: PgState | null = null;

  // A runner that provisions Postgres as a service container has no initdb, so
  // without this the CI job could only ever take the skip path.
  const provided = process.env.SITES_E2E_PG_URL?.trim();
  if (provided) {
    state = await provisionScratchDb(provided);
  } else {
    const pgTmpUrl = tryPgTmp();
    if (pgTmpUrl) {
      state = { kind: "pg_tmp", url: pgTmpUrl };
    } else {
      const cluster = tryCluster();
      if (cluster) state = cluster;
    }
  }

  if (!state) {
    console.warn(
      "[e2e] no postgres (SITES_E2E_PG_URL/pg_tmp/initdb) — the DB suites will " +
        "fail. Run inside `nix develop ./catalyrst/sites`, or point SITES_E2E_PG_URL at a " +
        "server, to exercise the DB path.",
    );
    process.env.SITES_E2E_PG = "skip";
    return;
  }

  // ALLOW_SKIPPED_INTEGRATION deliberately does not cover this: a server you
  // explicitly pointed the suite at must work.
  try {
    await applySchemaAndSeed(state.url);
  } catch (e) {
    throw new Error(
      `integration dependency configured but unusable: ${state.kind} postgres\n` +
        `  ${(e as Error).message}\n` +
        "  ALLOW_SKIPPED_INTEGRATION does not cover this: a dependency you " +
        "explicitly pointed the suite at must work.",
    );
  }

  process.env.CATALYST_DATABASE_URL = state.url;
  process.env.TELEMETRY_DATABASE_URL = state.url;
  process.env.SITES_E2E_PG = "ready";
  writeFileSync(STATE_FILE, JSON.stringify(state), "utf8");
}

export async function teardown(): Promise<void> {
  if (!existsSync(STATE_FILE)) return;
  let state: PgState;
  try {
    state = JSON.parse(readFileSync(STATE_FILE, "utf8")) as PgState;
  } catch {
    return;
  }
  try {
    if (state.kind === "provided") {
      const admin = new Pool({ connectionString: state.adminUrl, max: 1 });
      try {
        await admin.query(`DROP DATABASE IF EXISTS "${state.dbname}" WITH (FORCE)`);
      } finally {
        await admin.end();
      }
    }
    if (state.kind === "cluster") {
      spawnSync("pg_ctl", ["-D", state.datadir, "-m", "immediate", "stop"], {
        stdio: "ignore",
        env: pgSpawnEnv(),
      });
      rmSync(join(state.datadir, ".."), { recursive: true, force: true });
    }
  } finally {
    rmSync(STATE_FILE, { force: true });
  }
}
