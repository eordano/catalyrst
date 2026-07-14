import { describe, expect, it, vi } from "vitest";

import { loadProjectDetail } from "./project-detail";

const KNOWN_ID = "1f5c2a9e-0b3d-4c7a-9e21-7f8a1c2b3d4e";

function jsonResponse(body: unknown, ok = true): Response {
  return {
    ok,
    status: ok ? 200 : 404,
    json: async () => body,
  } as unknown as Response;
}

const liveDetail = {
  id: KNOWN_ID,
  proposal_id: "prop-1",
  title: "Live Detail Title",
  status: "finished",
  type: "grant",
  author: "0xabcdef0123456789abcdef0123456789abcdef01",
  about: "Live about text.",
  vesting_addresses: ["0xvest1", "0xvest2"],
  links: [],
  personnel: [{ name: "Team", about: "We build things." }],
  milestones: [
    {
      title: "M1",
      description: "do the thing",
      delivery_date: "2025-01-01T00:00:00.000Z",
    },
  ],
  updates: [{ id: "u1", status: "done" }],
  funding: {
    enacted_at: "2024-01-01T00:00:00.000Z",
    vesting: {
      start_at: "2024-01-01T00:00:00.000Z",
      finish_at: "2024-12-31T00:00:00.000Z",
      released: 25000,
      vested: 50000,
      total: 100000,
      token: "DAI",
    },
  },
};

describe("loadProjectDetail", () => {
  it("returns null for a missing id (never queries the backend)", async () => {
    const fetchImpl = vi.fn();
    const res = await loadProjectDetail(undefined, { fetchImpl });
    expect(res).toBeNull();
    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it("adapts the per-id detail document into the view model", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse(liveDetail));
    const res = await loadProjectDetail(KNOWN_ID, { fetchImpl });

    expect(res!.source).toBe("live");
    const p = res!.project!;
    expect(p.title).toBe("Live Detail Title");
    expect(p.about).toBe("Live about text.");
    expect(p.funding.vestedPct).toBe(50);
    expect(p.funding.releasedPct).toBe(25);
    expect(p.funding.vestedAmount).toBe("50,000");
    expect(p.funding.token).toBe("DAI");
    expect(p.vestings.length).toBe(2);
    expect(p.vestings[0].url).toContain("0xvest1");
    expect(p.milestones.length).toBe(1);
    expect(p.milestones[0].date).toBe("2025-01-01");
    expect(p.personnel[0].name).toBe("Team");
    expect(p.authorLabel).toMatch(/^0xabcd\u2026ef01$/);
  });

  it("accepts a { data } enveloped detail body", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({ data: liveDetail }));
    const res = await loadProjectDetail(KNOWN_ID, { fetchImpl });
    expect(res!.source).toBe("live");
    expect(res!.project!.title).toBe("Live Detail Title");
  });

  it("falls back to the list endpoint when the per-id route 404s", async () => {
    const summary = {
      id: KNOWN_ID,
      title: "List Summary",
      status: "finished",
      vesting_addresses: ["0xvest1"],
    };
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith(`/projects/${KNOWN_ID}`)) return jsonResponse({}, false);
      return jsonResponse({ data: [summary] });
    });
    const res = await loadProjectDetail(KNOWN_ID, { fetchImpl });
    expect(res!.source).toBe("live");
    expect(res!.project!.title).toBe("List Summary");
    expect(res!.project!.milestones).toEqual([]);
    expect(res!.project!.personnel).toEqual([]);
    expect(res!.project!.vestings.length).toBe(1);
  });

  it("returns an explicit fallback (NOT a fixture) when the backend is unreachable", async () => {
    const fetchImpl = vi.fn(async () => {
      throw new Error("network down");
    });
    const res = await loadProjectDetail(KNOWN_ID, { fetchImpl });
    expect(res).not.toBeNull();
    expect(res!.source).toBe("fallback");
    expect(res!.project).toBeNull();
  });

  it("returns a fallback when the backend returns junk JSON", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({ nope: true }));
    const res = await loadProjectDetail(KNOWN_ID, { fetchImpl });
    expect(res!.source).toBe("fallback");
    expect(res!.project).toBeNull();
  });
});
