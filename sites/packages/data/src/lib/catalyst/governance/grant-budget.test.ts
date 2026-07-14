import { describe, expect, it, vi } from "vitest";

import { loadGrantBudget } from "./grant-budget";

const BASE = "http://gov.test";

/** One period, in the exact wire shape GET /budgets serves (BudgetRow). */
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
  it("calls /budgets \u{2014} the path this node routes \u{2014} not the upstream /budget/all", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse(PERIODS));
    await loadGrantBudget({ base: BASE, fetchImpl: fetchImpl as never });
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fetchImpl.mock.calls[0][0]).toBe(`${BASE}/budgets`);
  });

  it("projects the newest period and reports how fresh it is", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse(PERIODS));
    const budget = await loadGrantBudget({ base: BASE, fetchImpl: fetchImpl as never });
    expect(budget.source).toBe("live");
    expect(budget.period.id).toBe("newest");
    expect(budget.asOf).toBe("2025-01-01T00:00:00.000Z");
    const platform = budget.categories.find((c) => c.key === "platform");
    expect(platform?.available).toBe(325174);
  });

  it("reports an unavailable state on a non-2xx \u{2014} it must not serve the fixture", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse({ error: "nope" }, 404));
    const budget = await loadGrantBudget({ base: BASE, fetchImpl: fetchImpl as never });
    expect(budget.source).toBe("unavailable");
    expect(budget.reason).toMatch(/404/);
    expect(budget.categories).toEqual([]);
  });

  it("reports an unavailable state when the endpoint is unreachable", async () => {
    const fetchImpl = vi.fn(async (_url: string) => {
      throw new Error("ECONNREFUSED");
    });
    const budget = await loadGrantBudget({ base: BASE, fetchImpl: fetchImpl as never });
    expect(budget.source).toBe("unavailable");
    expect(budget.reason).toMatch(/ECONNREFUSED/);
  });

  it("reports an unavailable state when the node holds no budget periods", async () => {
    const fetchImpl = vi.fn(async (_url: string) =>
      jsonResponse({ data: [], limit: 100, offset: 0 }),
    );
    const budget = await loadGrantBudget({ base: BASE, fetchImpl: fetchImpl as never });
    expect(budget.source).toBe("unavailable");
    expect(budget.reason).toMatch(/no budget periods/);
  });
});
