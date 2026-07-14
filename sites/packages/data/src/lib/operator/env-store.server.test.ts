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
  it("is unavailable with a fix when the path is not configured", async () => {
    vi.stubEnv("CATALYRST_OPERATOR_ENV_FILE", "");
    const r = await readOperatorEnv();
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.reason).toBe("not-configured");
      expect(r.fix).toContain("CATALYRST_OPERATOR_ENV_FILE");
    }
  });

  it("treats a missing file as an empty store, not an error", async () => {
    const r = await readOperatorEnv();
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.data.entries).toEqual([]);
  });

  it("parses NAME=value lines and counts foreign lines as preserved", async () => {
    await writeFile(file, "# hand comment\nFOO=bar\nlowercase=skipped\n");
    const r = await readOperatorEnv();
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.data.entries).toEqual([{ name: "FOO", value: "bar" }]);
      expect(r.data.preservedLines).toBe(2);
    }
  });
});

describe("upsertOperatorEnv", () => {
  it("creates the file, appends, and updates in place", async () => {
    await upsertOperatorEnv("A_ONE", "1");
    await upsertOperatorEnv("B_TWO", "2");
    const updated = await upsertOperatorEnv("A_ONE", "1b");
    expect(updated.ok).toBe(true);
    if (updated.ok) {
      expect(updated.data.entries).toEqual([
        { name: "A_ONE", value: "1b" },
        { name: "B_TWO", value: "2" },
      ]);
    }
  });

  it("preserves hand-written lines verbatim", async () => {
    await writeFile(file, "# keep me\nFOO=old\n");
    await upsertOperatorEnv("FOO", "new");
    const raw = await readFile(file, "utf8");
    expect(raw).toBe("# keep me\nFOO=new\n");
  });

  it("keeps values with = and spaces whole", async () => {
    await upsertOperatorEnv("DSN_LIKE", "postgres://u:p@h/db?a=1 b=2");
    const r = await readOperatorEnv();
    if (r.ok) expect(r.data.entries[0].value).toBe("postgres://u:p@h/db?a=1 b=2");
  });

  it("rejects bad names and multi-line values without touching the file", async () => {
    const bad = await upsertOperatorEnv("lower", "x");
    expect(bad.ok).toBe(false);
    const multi = await upsertOperatorEnv("GOOD_NAME", "a\nb");
    expect(multi.ok).toBe(false);
    const r = await readOperatorEnv();
    if (r.ok) expect(r.data.entries).toEqual([]);
  });
});

describe("removeOperatorEnv", () => {
  it("removes exactly the named entry", async () => {
    await writeFile(file, "# note\nFOO=1\nBAR=2\n");
    const r = await removeOperatorEnv("FOO");
    expect(r.ok).toBe(true);
    const raw = await readFile(file, "utf8");
    expect(raw).toBe("# note\nBAR=2\n");
  });
});
