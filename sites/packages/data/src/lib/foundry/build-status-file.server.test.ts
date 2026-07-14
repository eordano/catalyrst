import { promises as fs } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { readBuildStatusFile } from "./build-status-file.server";

// The on-disk shape the Forge broker drops while it works:
// projects/<slug>/build/status.json.

async function withRoot(fn: (root: string) => Promise<void>): Promise<void> {
  const root = await fs.mkdtemp(join(tmpdir(), "fd-build-status-"));
  try {
    await fn(root);
  } finally {
    await fs.rm(root, { recursive: true, force: true });
  }
}

async function writeStatus(root: string, slug: string, body: string): Promise<void> {
  const dir = join(root, "projects", slug, "build");
  await fs.mkdir(dir, { recursive: true });
  await fs.writeFile(join(dir, "status.json"), body, "utf8");
}

describe("readBuildStatusFile", () => {
  it("reads step, note and updated from a stored file", async () => {
    await withRoot(async (root) => {
      await writeStatus(
        root,
        "zoo-run",
        JSON.stringify({
          step: "vitest",
          note: "14 passing",
          updated: "2026-08-22T10:00:00.000Z",
        }),
      );
      expect(await readBuildStatusFile("zoo-run", root)).toEqual({
        step: "vitest",
        note: "14 passing",
        updated: "2026-08-22T10:00:00.000Z",
      });
    });
  });

  it("tolerates a missing file, malformed JSON and a step-less object as null", async () => {
    await withRoot(async (root) => {
      expect(await readBuildStatusFile("absent", root)).toBeNull();
      await writeStatus(root, "broken", "{not json");
      expect(await readBuildStatusFile("broken", root)).toBeNull();
      await writeStatus(root, "stepless", JSON.stringify({ note: "no step" }));
      expect(await readBuildStatusFile("stepless", root)).toBeNull();
    });
  });

  it("refuses a slug outside the gate before touching the disk", async () => {
    await withRoot(async (root) => {
      expect(await readBuildStatusFile("../escape", root)).toBeNull();
      expect(await readBuildStatusFile("UPPER", root)).toBeNull();
      expect(await readBuildStatusFile("", root)).toBeNull();
    });
  });
});
