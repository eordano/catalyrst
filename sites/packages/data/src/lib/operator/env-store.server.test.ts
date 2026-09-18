import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  readOperatorEnv,
  removeOperatorEnv,
  upsertOperatorEnv,
} from "./env-store.server";

let dir: string;
let file: string;

beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), "operator-env-"));
  file = join(dir, "operator.env");
  vi.stubEnv("CATALYRST_OPERATOR_ENV_FILE", file);
});

afterEach(async () => {
  vi.unstubAllEnvs();
  await rm(dir, { recursive: true, force: true });
});

describe("readOperatorEnv", () => {
  it("is unavailable with a fix when unconfigured, treats a missing file as an empty store, and parses NAME=value lines counting foreign lines as preserved", async () => {
    const missing = await readOperatorEnv();
    expect(missing.ok).toBe(true);
    if (missing.ok) expect(missing.data.entries).toEqual([]);

    await writeFile(file, "# hand comment\nFOO=bar\nlowercase=skipped\n");
    const parsed = await readOperatorEnv();
    expect(parsed.ok).toBe(true);
    if (parsed.ok) {
      expect(parsed.data.entries).toEqual([{ name: "FOO", value: "bar" }]);
      expect(parsed.data.preservedLines).toBe(2);
    }

    vi.stubEnv("CATALYRST_OPERATOR_ENV_FILE", "");
    const r = await readOperatorEnv();
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.reason).toBe("not-configured");
      expect(r.fix).toContain("CATALYRST_OPERATOR_ENV_FILE");
    }
  });
});

describe("upsertOperatorEnv", () => {
  it("creates the file, appends, updates in place, and keeps values with = and spaces whole", async () => {
    await upsertOperatorEnv("A_ONE", "1");
    await upsertOperatorEnv("B_TWO", "2");
    await upsertOperatorEnv("DSN_LIKE", "postgres://u:p@h/db?a=1 b=2");
    const updated = await upsertOperatorEnv("A_ONE", "1b");
    expect(updated.ok).toBe(true);
    if (updated.ok) {
      expect(updated.data.entries).toEqual([
        { name: "A_ONE", value: "1b" },
        { name: "B_TWO", value: "2" },
        { name: "DSN_LIKE", value: "postgres://u:p@h/db?a=1 b=2" },
      ]);
    }
  });

  it("preserves hand-written lines verbatim on update, and remove takes exactly the named entry", async () => {
    await writeFile(file, "# keep me\nFOO=old\nBAR=2\n");
    await upsertOperatorEnv("FOO", "new");
    expect(await readFile(file, "utf8")).toBe("# keep me\nFOO=new\nBAR=2\n");

    const r = await removeOperatorEnv("FOO");
    expect(r.ok).toBe(true);
    expect(await readFile(file, "utf8")).toBe("# keep me\nBAR=2\n");
  });

  it("rejects bad names and multi-line values without touching the file", async () => {
    const bad = await upsertOperatorEnv("lower", "x");
    expect(bad.ok).toBe(false);
    const multi = await upsertOperatorEnv("GOOD_NAME", "a\nb");
    expect(multi.ok).toBe(false);
    const r = await readOperatorEnv();
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.data.entries).toEqual([]);
  });
});
