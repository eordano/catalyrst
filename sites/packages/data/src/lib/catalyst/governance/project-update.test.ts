import { describe, expect, it, vi } from "vitest";

import { loadProjectUpdateContext } from "./project-update";
import { loadEditUpdate } from "./edit-project-update";

const BASE = "http://gov.test";
const ID = "p-1";

/**
 * GET /projects/{id} as catalyrst-governance builds it: the project row with
 * "updates" spliced in (parse.rs:151).
 */
const DETAIL = {
  id: ID,
  proposal_id: "prop-1",
  title: "A project",
  type: "grant",
  status: "in_progress",
  author: "0xauthor",
  configuration: { category: "Platform" },
  funding: { vesting: { total: 100, logs: [] } },
  updates: [
    {
      id: "u-1",
      project_id: ID,
      proposal_id: "prop-1",
      health: "onTrack",
      introduction: "first",
      highlights: "h1",
      blockers: "",
      next_steps: "n1",
      status: "done",
      completion_date: "2024-06-01T00:00:00.000Z",
      created_at: "2024-06-01T00:00:00.000Z",
    },
    {
      id: "u-2",
      project_id: ID,
      proposal_id: "prop-1",
      health: "atRisk",
      introduction: "second",
      highlights: "h2",
      blockers: "b2",
      next_steps: "n2",
      status: "done",
      completion_date: "2024-09-01T00:00:00.000Z",
      created_at: "2024-09-01T00:00:00.000Z",
    },
  ],
};

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("loadProjectUpdateContext", () => {
  it("reads the nested updates from /projects/{id} \u{2014} this node serves no /updates route", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse(DETAIL));
    const ctx = await loadProjectUpdateContext(ID, {
      base: BASE,
      fetchImpl: fetchImpl as never,
    });
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fetchImpl.mock.calls[0][0]).toBe(`${BASE}/projects/${ID}`);
    expect(ctx.source).toBe("live");
    expect(ctx.project.id).toBe(ID);
    expect(ctx.priorUpdates.map((u) => u.id)).toEqual(["u-2", "u-1"]);
  });

  it("reports an unavailable state on 404 instead of serving the fixture project", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse({ error: "Not Found" }, 404));
    const ctx = await loadProjectUpdateContext(ID, {
      base: BASE,
      fetchImpl: fetchImpl as never,
    });
    expect(ctx.source).toBe("unavailable");
    expect(ctx.reason).toMatch(/no project with that id/);
    expect(ctx.project.title).toBe("");
    expect(ctx.priorUpdates).toEqual([]);
  });

  it("reports an unavailable state when the endpoint is unreachable", async () => {
    const fetchImpl = vi.fn(async (_url: string) => {
      throw new Error("ECONNREFUSED");
    });
    const ctx = await loadProjectUpdateContext(ID, {
      base: BASE,
      fetchImpl: fetchImpl as never,
    });
    expect(ctx.source).toBe("unavailable");
    expect(ctx.reason).toMatch(/ECONNREFUSED/);
  });
});

describe("loadEditUpdate", () => {
  it("edits the newest nested update from /projects/{id}", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse(DETAIL));
    const data = await loadEditUpdate(ID, {
      base: BASE,
      fetchImpl: fetchImpl as never,
    });
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fetchImpl.mock.calls[0][0]).toBe(`${BASE}/projects/${ID}`);
    expect(data.source).toBe("live");
    expect(data.update.id).toBe("u-2");
    expect(data.project.title).toBe("A project");
  });

  it("reports an unavailable state rather than opening an editor onto fixture text", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse({ ...DETAIL, updates: [] }));
    const data = await loadEditUpdate(ID, {
      base: BASE,
      fetchImpl: fetchImpl as never,
    });
    expect(data.source).toBe("unavailable");
    expect(data.reason).toMatch(/no published update/);
    expect(data.update.introduction).toBe("");
  });
});
