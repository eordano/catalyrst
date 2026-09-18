import { describe, expect, it } from "vitest";

import fixture from "../../../fixtures/governance-projects.json";
import {
  mapProject,
  filterProjects,
  computeStats,
  fundingYears,
  normalizeCategory,
  normalizeStatus,
  loadProjects,
  type ProjectInList,
  type ProjectFilters,
} from "./projects";

const NOW = Date.parse("2026-06-23T00:00:00.000Z");

const ROWS = (fixture as { data: unknown[] }).data;

const listProjectsFromFixture = (now: number) =>
  (ROWS as ProjectInList[]).map((r) => mapProject(r, now));

const NO_FILTERS: ProjectFilters = {
  category: "",
  subtype: "",
  status: "",
  year: "",
  quarter: "",
  sort: "update_timestamp",
};

describe("fixture shape + view-model", () => {
  it("carries _source provenance and maps every fixture row to a ui3 ProjectCard", () => {
    expect((fixture as { _source?: string })._source).toMatch(/decentraland\/governance/);
    const cards = listProjectsFromFixture(NOW);
    expect(cards.length).toBe(ROWS.length);
    for (const c of cards) {
      expect(typeof c.id).toBe("string");
      expect(["grant", "bid"]).toContain(c.type);
      expect(c.vestedPct).toBeGreaterThanOrEqual(0);
      expect(c.vestedPct).toBeLessThanOrEqual(100);
      expect(c.releasedPct).toBeLessThanOrEqual(c.vestedPct + 1);
      expect(typeof c.update.intro).toBe("string");
    }
  });

  it("derives category keys (bid -> tender), shortens raw 0x authors, and labels finished projects 'ended'", () => {
    const cards = listProjectsFromFixture(NOW);
    const bid = cards.find((c) => c.type === "bid");
    expect(bid?.category).toBe("tender");
    const platform = cards.find((c) => c.title.includes("WebGPU"));
    expect(platform?.category).toBe("platform");
    expect(cards[0].author).toMatch(/^0x[0-9a-f]{4}\u2026[0-9a-f]{4}$/i);
    const finished = cards.find((c) => c.status === "finished");
    expect(finished?.ends.toLowerCase()).toContain("ended");
  });
});

describe("filterProjects (URL-addressable)", () => {
  const all = listProjectsFromFixture(NOW);

  it("category=grants excludes bids, category=bidding keeps only bids, and a grant subtype narrows to one category", () => {
    const grants = filterProjects(all, { ...NO_FILTERS, category: "grants" });
    expect(grants.every((p) => p.type === "grant")).toBe(true);

    const bids = filterProjects(all, { ...NO_FILTERS, category: "bidding" });
    expect(bids.length).toBeGreaterThan(0);
    expect(bids.every((p) => p.type === "bid")).toBe(true);

    const platform = filterProjects(all, { ...NO_FILTERS, category: "grants", subtype: "platform" });
    expect(platform.every((p) => p.category === "platform")).toBe(true);
  });

  it("status=ongoing aliases to in_progress and status=finished filters", () => {
    const ongoing = filterProjects(all, { ...NO_FILTERS, status: "ongoing" });
    expect(ongoing.length).toBeGreaterThan(0);
    expect(ongoing.every((p) => p.status === "in_progress")).toBe(true);

    const finished = filterProjects(all, { ...NO_FILTERS, status: "finished" });
    expect(finished.every((p) => p.status === "finished")).toBe(true);
  });

  it("year + quarter narrows by funding quarter and sort=size orders by descending requested amount", () => {
    const q1 = filterProjects(all, { ...NO_FILTERS, year: "2026", quarter: "Q1" });
    expect(q1.every((p) => p.quarter === "2026-Q1")).toBe(true);

    const bySize = filterProjects(all, { ...NO_FILTERS, sort: "size" });
    for (let i = 1; i < bySize.length; i++) {
      expect(bySize[i - 1].size).toBeGreaterThanOrEqual(bySize[i].size);
    }
  });
});

describe("normalizers", () => {
  it("normalizeCategory and normalizeStatus resolve their URL aliases", () => {
    expect(normalizeCategory("grants")).toBe("grants");
    expect(normalizeCategory("bidding")).toBe("bidding_and_tendering");
    expect(normalizeCategory("")).toBe("");
    expect(normalizeStatus("ongoing")).toBe("in_progress");
    expect(normalizeStatus("all")).toBe("");
    expect(normalizeStatus("")).toBe("");
  });
});

describe("stats + years", () => {
  const all = listProjectsFromFixture(NOW);
  it("computeStats sums funding per type and fundingYears is descending + distinct", () => {
    const s = computeStats(all);
    expect(s.count).toBe(all.length);
    expect(s.grantsCount + s.bidsCount).toBe(all.length);
    expect(s.totalFunding).toBe(s.grantFunding + s.bidFunding);
    expect(s.ongoingCount).toBeGreaterThan(0);

    const ys = fundingYears(all);
    expect(ys.length).toBeGreaterThan(0);
    for (let i = 1; i < ys.length; i++) expect(ys[i - 1]).toBeGreaterThan(ys[i]);
  });
});

describe("loadProjects (best-effort live -> honest-empty fallback)", () => {
  it("reports source \"unavailable\" on a non-2xx or a network error (never the fixture) and uses live data for a valid non-empty envelope", async () => {
    const nonOk: typeof fetch = async () =>
      new Response("nope", { status: 502 }) as unknown as Response;
    const bad = await loadProjects({ fetchImpl: nonOk }, NOW);
    expect(bad.source).toBe("unavailable");
    expect(bad.projects.length).toBe(0);
    expect(bad.asOf).toBeNull();

    const thrown: typeof fetch = async () => {
      throw new Error("network down");
    };
    const down = await loadProjects({ fetchImpl: thrown }, NOW);
    expect(down.source).toBe("unavailable");
    expect(down.projects.length).toBe(0);
    expect(down.asOf).toBeNull();

    const live = {
      data: [
        {
          id: "live-1",
          proposal_id: "0xlive",
          title: "Live grant project",
          status: "in_progress",
          type: "grant",
          author: "0xabcabcabcabcabcabcabcabcabcabcabcabcabca",
          configuration: { category: "Platform", size: 50000 },
          funding: {
            enacted_at: "2026-04-01T00:00:00.000Z",
            vesting: { total: 50000, vested: 25000, released: 20000, releasable: 5000, token: "USDC", status: "In Progress", start_at: "2026-04-01T00:00:00.000Z", finish_at: "2026-10-01T00:00:00.000Z" },
          },
          latest_update: { update: { index: 1, introduction: "Kickoff.", health: "onTrack", completion_date: "2026-05-01T00:00:00.000Z" }, update_timestamp: 1772000000 },
          created_at: 1770000000,
          updated_at: 1772000000,
        } satisfies ProjectInList,
      ],
      limit: 200,
      offset: 0,
    };
    const ok: typeof fetch = async () =>
      new Response(JSON.stringify(live), { status: 200 }) as unknown as Response;
    const { projects, source } = await loadProjects({ fetchImpl: ok }, NOW);
    expect(source).toBe("live");
    expect(projects).toHaveLength(1);
    expect(projects[0].id).toBe("live-1");
    expect(projects[0].category).toBe("platform");
    expect(projects[0].vestedPct).toBe(50);
  });
});
