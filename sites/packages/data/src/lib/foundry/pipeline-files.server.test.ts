import { promises as fs } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { listPipelines, readPipeline } from "./pipeline-files.server";

// The on-disk shape the docsmith MCP server writes:
// projects/<slug>/pipeline/pipeline.json plus the step artifacts it names.

async function writePipeline(
  root: string,
  slug: string,
  body: Record<string, unknown>,
): Promise<void> {
  const dir = join(root, "projects", slug, "pipeline");
  await fs.mkdir(dir, { recursive: true });
  await fs.writeFile(join(dir, "pipeline.json"), JSON.stringify(body, null, 2) + "\n", "utf8");
}

function steps(overrides: Partial<Record<string, unknown>>[] = []) {
  const base = [
    { id: "intake", status: "pending", artifact: null, problems: [], updated: null },
    { id: "interview", status: "pending", artifact: null, problems: [], updated: null },
    { id: "draft", status: "pending", artifact: null, problems: [], updated: null },
    { id: "evidence", status: "pending", artifact: null, problems: [], updated: null },
  ];
  return base.map((step, i) => ({ ...step, ...overrides[i] }));
}

async function withRoot(fn: (root: string) => Promise<void>): Promise<void> {
  const root = await fs.mkdtemp(join(tmpdir(), "fd-pipelines-"));
  try {
    await fn(root);
  } finally {
    await fs.rm(root, { recursive: true, force: true });
  }
}

describe("listPipelines", () => {
  it("maps summary fields and picks the first non-passed step as next", async () => {
    await withRoot(async (root) => {
      await writePipeline(root, "midnight-couriers", {
        slug: "midnight-couriers",
        title: "Midnight Couriers",
        kind: "shortgdd",
        created: "2026-08-20T14:02:11.000Z",
        steps: steps([
          { status: "passed", artifact: "projects/midnight-couriers/pipeline/01-intake.md", updated: "2026-08-20T14:05:00.000Z" },
          { status: "failed", problems: ["only 3 decision rows — the gate needs at least 5"], updated: "2026-08-20T14:09:00.000Z" },
        ]),
      });

      const rows = await listPipelines(root);
      expect(rows).toEqual([
        {
          slug: "midnight-couriers",
          title: "Midnight Couriers",
          kind: "shortgdd",
          created: "2026-08-20T14:02:11.000Z",
          passed: 1,
          total: 4,
          next: "interview",
        },
      ]);
    });
  });

  it("reports next null when every step passed", async () => {
    await withRoot(async (root) => {
      await writePipeline(root, "done-run", {
        slug: "done-run",
        title: "Done Run",
        kind: "shortgdd",
        created: "2026-08-19T10:00:00.000Z",
        steps: steps([
          { status: "passed" },
          { status: "passed" },
          { status: "passed" },
          { status: "passed" },
        ]),
      });
      const rows = await listPipelines(root);
      expect(rows[0]?.next).toBeNull();
      expect(rows[0]?.passed).toBe(4);
    });
  });

  it("sorts by created descending", async () => {
    await withRoot(async (root) => {
      await writePipeline(root, "older", {
        slug: "older",
        title: "Older",
        kind: "shortgdd",
        created: "2026-08-18T00:00:00.000Z",
        steps: steps(),
      });
      await writePipeline(root, "newer", {
        slug: "newer",
        title: "Newer",
        kind: "shortgdd",
        created: "2026-08-20T00:00:00.000Z",
        steps: steps(),
      });
      const rows = await listPipelines(root);
      expect(rows.map((r) => r.slug)).toEqual(["newer", "older"]);
    });
  });

  it("returns [] when the projects dir is missing", async () => {
    await withRoot(async (root) => {
      expect(await listPipelines(root)).toEqual([]);
    });
  });

  it("skips malformed pipeline.json and projects without one", async () => {
    await withRoot(async (root) => {
      await writePipeline(root, "good", {
        slug: "good",
        title: "Good",
        kind: "shortgdd",
        created: "2026-08-20T00:00:00.000Z",
        steps: steps(),
      });
      const brokenDir = join(root, "projects", "broken", "pipeline");
      await fs.mkdir(brokenDir, { recursive: true });
      await fs.writeFile(join(brokenDir, "pipeline.json"), "{not json", "utf8");
      await writePipeline(root, "no-steps", {
        slug: "no-steps",
        title: "No Steps",
      });
      await fs.mkdir(join(root, "projects", "bare"), { recursive: true });

      const rows = await listPipelines(root);
      expect(rows.map((r) => r.slug)).toEqual(["good"]);
    });
  });
});

describe("readPipeline", () => {
  it("returns the full detail with artifact content", async () => {
    await withRoot(async (root) => {
      const artifact = "projects/midnight-couriers/pipeline/01-intake.md";
      await writePipeline(root, "midnight-couriers", {
        slug: "midnight-couriers",
        title: "Midnight Couriers",
        kind: "shortgdd",
        created: "2026-08-20T14:02:11.000Z",
        steps: steps([
          { status: "passed", artifact, updated: "2026-08-20T14:05:00.000Z" },
        ]),
      });
      await fs.writeFile(join(root, artifact), "## Concept\n\nA courier game.\n", "utf8");

      const detail = await readPipeline("midnight-couriers", root);
      expect(detail?.title).toBe("Midnight Couriers");
      expect(detail?.steps).toHaveLength(4);
      expect(detail?.steps[0]).toEqual({
        id: "intake",
        status: "passed",
        artifact,
        problems: [],
        updated: "2026-08-20T14:05:00.000Z",
        content: "## Concept\n\nA courier game.\n",
      });
      expect(detail?.steps[1]?.content).toBeNull();
    });
  });

  it("returns content null when the artifact file is missing", async () => {
    await withRoot(async (root) => {
      await writePipeline(root, "ghost", {
        slug: "ghost",
        title: "Ghost",
        kind: "shortgdd",
        created: "2026-08-20T00:00:00.000Z",
        steps: steps([
          { status: "passed", artifact: "projects/ghost/pipeline/01-intake.md" },
        ]),
      });
      const detail = await readPipeline("ghost", root);
      expect(detail?.steps[0]?.status).toBe("passed");
      expect(detail?.steps[0]?.content).toBeNull();
    });
  });

  it("returns content null for an artifact path escaping the root", async () => {
    await withRoot(async (root) => {
      await writePipeline(root, "escape", {
        slug: "escape",
        title: "Escape",
        kind: "shortgdd",
        created: "2026-08-20T00:00:00.000Z",
        steps: steps([{ status: "passed", artifact: "../../../etc/hostname" }]),
      });
      const detail = await readPipeline("escape", root);
      expect(detail?.steps[0]?.content).toBeNull();
    });
  });

  it("returns null for unknown slug, malformed JSON, and traversal slugs", async () => {
    await withRoot(async (root) => {
      const brokenDir = join(root, "projects", "broken", "pipeline");
      await fs.mkdir(brokenDir, { recursive: true });
      await fs.writeFile(join(brokenDir, "pipeline.json"), "{not json", "utf8");

      expect(await readPipeline("nobody", root)).toBeNull();
      expect(await readPipeline("broken", root)).toBeNull();
      expect(await readPipeline("../etc", root)).toBeNull();
    });
  });
});
