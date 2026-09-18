import { describe, expect, it, vi } from "vitest";

import { loadProjectUpdateContext } from "./project-update";
import { loadEditUpdate } from "./edit-project-update";

const BASE = "http://gov.test";
const ID = "p-1";

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
  it("reads the nested updates from /projects/{id} (this node serves no /updates route) and reports unavailable on 404 or an unreachable endpoint instead of the fixture project", async () => {
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

    const notFound = vi.fn(async (_url: string) => jsonResponse({ error: "Not Found" }, 404));
    const missing = await loadProjectUpdateContext(ID, {
      base: BASE,
      fetchImpl: notFound as never,
    });
    expect(missing.source).toBe("unavailable");
    expect(missing.reason).toMatch(/no project with that id/);
    expect(missing.project.title).toBe("");
    expect(missing.priorUpdates).toEqual([]);

    const unreachable = vi.fn(async (_url: string) => {
      throw new Error("ECONNREFUSED");
    });
    const down = await loadProjectUpdateContext(ID, {
      base: BASE,
      fetchImpl: unreachable as never,
    });
    expect(down.source).toBe("unavailable");
    expect(down.reason).toMatch(/ECONNREFUSED/);
  });
});

describe("loadEditUpdate", () => {
  it("edits the newest nested update from /projects/{id} and reports unavailable rather than opening an editor onto fixture text", async () => {
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

    const noUpdates = vi.fn(async (_url: string) => jsonResponse({ ...DETAIL, updates: [] }));
    const empty = await loadEditUpdate(ID, {
      base: BASE,
      fetchImpl: noUpdates as never,
    });
    expect(empty.source).toBe("unavailable");
    expect(empty.reason).toMatch(/no published update/);
    expect(empty.update.introduction).toBe("");
  });
});
