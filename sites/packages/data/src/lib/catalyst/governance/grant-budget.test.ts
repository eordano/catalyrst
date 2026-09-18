import { describe, expect, it, vi } from "vitest";

import { loadGrantBudget } from "./grant-budget";

const BASE = "http://gov.test";

const PERIODS = {
  data: [
    {
      id: "old",
      start_at: "2024-07-01T00:00:00.000Z",
      finish_at: "2024-10-01T00:00:00.000Z",
      total: 100,
      allocated: 10,
      categories: { platform: { total: 100, allocated: 10, available: 90 } },
    },
    {
      id: "newest",
      start_at: "2024-10-01T00:00:00.000Z",
      finish_at: "2025-01-01T00:00:00.000Z",
      total: 699237,
      allocated: 257500,
      categories: {
        platform: { total: 582674, allocated: 257500, available: 325174 },
        core_unit: { total: 116563, allocated: 0, available: 116563 },
      },
    },
  ],
  limit: 100,
  offset: 0,
};

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("loadGrantBudget", () => {
  it("calls /budgets (the path this node routes, not the upstream /budget/all) and projects the newest period with its freshness", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse(PERIODS));
    const budget = await loadGrantBudget({ base: BASE, fetchImpl: fetchImpl as never });
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fetchImpl.mock.calls[0][0]).toBe(`${BASE}/budgets`);
    expect(budget.source).toBe("live");
    expect(budget.period.id).toBe("newest");
    expect(budget.asOf).toBe("2025-01-01T00:00:00.000Z");
    const platform = budget.categories.find((c) => c.key === "platform");
    expect(platform?.available).toBe(325174);
  });

  it("reports an unavailable state (never the fixture) on a non-2xx, an unreachable endpoint, or a node with no budget periods", async () => {
    const nonOk = vi.fn(async (_url: string) => jsonResponse({ error: "nope" }, 404));
    const notFound = await loadGrantBudget({ base: BASE, fetchImpl: nonOk as never });
    expect(notFound.source).toBe("unavailable");
    expect(notFound.reason).toMatch(/404/);
    expect(notFound.categories).toEqual([]);

    const unreachable = vi.fn(async (_url: string) => {
      throw new Error("ECONNREFUSED");
    });
    const down = await loadGrantBudget({ base: BASE, fetchImpl: unreachable as never });
    expect(down.source).toBe("unavailable");
    expect(down.reason).toMatch(/ECONNREFUSED/);

    const empty = vi.fn(async (_url: string) =>
      jsonResponse({ data: [], limit: 100, offset: 0 }),
    );
    const none = await loadGrantBudget({ base: BASE, fetchImpl: empty as never });
    expect(none.source).toBe("unavailable");
    expect(none.reason).toMatch(/no budget periods/);
  });
});
