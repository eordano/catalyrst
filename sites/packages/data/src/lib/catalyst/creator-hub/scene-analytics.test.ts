import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  SCENE_STATS_PATH,
  creatorsDataBase,
  fetchCreatorScenesStats,
  parseCreatorScenesStats,
} from "./scene-analytics";
import { signedGetJSON } from "../client";
import type { AuthIdentity } from "../../auth/types";

vi.mock("../client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../client")>();
  return { ...actual, signedGetJSON: vi.fn() };
});

const identity = { signer: "0xabc" } as unknown as AuthIdentity;

function wireScene(overrides: Record<string, unknown> = {}) {
  const wireWindow = {
    users: 120,
    new_users: 35,
    returning_users: 85,
    visits: 350,
    median_active_time_s: 152,
    total_active_hours: 15.3,
    afk_time_pct: 9.1,
    peak_concurrent_users: 27,
    median_fps: null,
    perf_sessions: 60,
    dau: 12,
    desktop_users: 108,
    mobile_users: 18,
    messages_sent: 820,
    emotes_played: 430,
    avg_session_active_time_s: 205,
  };
  return {
    scene_type: "world",
    scene_id: "kickoff.dcl.eth",
    title: "Kickoff",
    deployer_address: "0xdeployer",
    windows: {
      yesterday: wireWindow,
      last_7d: wireWindow,
      last_30d: wireWindow,
    },
    retention: { cohort: 429, d1: 18.0, d7: 30.5, d30: 35.2 },
    retention_series: [
      { date: "2026-07-20", cohort: 30, d1: 18, d7: 30.5, d30: null },
    ],
    daily: [
      {
        date: "2026-07-20",
        visits: 40,
        unique_users: 25,
        new_users: 4,
        median_active_time_s: 180,
        peak_concurrent_users: 8,
        messages_sent: 80,
        emotes_played: 42,
      },
    ],
    deploy_dates: ["2026-07-15"],
    ...overrides,
  };
}

describe("when resolving the creators-data base URL", () => {
  it("should default to the production mount and strip trailing slashes", () => {
    expect(creatorsDataBase()).toBe(
      "https://decentraland.org/creators-data/api",
    );
    expect(creatorsDataBase("http://localhost:8787/api/")).toBe(
      "http://localhost:8787/api",
    );
  });
});

describe("when parsing the wire payload", () => {
  it("should convert snake_case fields to the camelCase model", () => {
    const parsed = parseCreatorScenesStats({
      address: "0xabc",
      as_of: "2026-07-21",
      scenes: [wireScene()],
    });
    expect(parsed.address).toBe("0xabc");
    expect(parsed.asOf).toBe("2026-07-21");
    const scene = parsed.scenes[0];
    expect(scene.sceneType).toBe("world");
    expect(scene.sceneId).toBe("kickoff.dcl.eth");
    expect(scene.deployerAddress).toBe("0xdeployer");
    expect(scene.windows.last_7d.newUsers).toBe(35);
    expect(scene.windows.last_30d.medianActiveTimeS).toBe(152);
    expect(scene.windows.yesterday.avgSessionActiveTimeS).toBe(205);
    expect(scene.retention.d7).toBe(30.5);
    expect(scene.retentionSeries[0].d30).toBeNull();
    expect(scene.daily[0].uniqueUsers).toBe(25);
    expect(scene.deployDates).toEqual(["2026-07-15"]);
  });

  it("should reject a scene whose measurement collections are missing", () => {
    // These arrays are the measurements: the daily rows, the retention series
    // and the deploy dates a creator reads the page for. Defaulting them to []
    // drew an empty chart for a scene whose numbers were never in the payload,
    // which is the same picture as a scene nobody visited.
    expect(() =>
      parseCreatorScenesStats({
        address: "0xabc",
        as_of: "2026-07-21",
        scenes: [
          wireScene({
            retention_series: undefined,
            daily: undefined,
            deploy_dates: undefined,
          }),
        ],
      }),
    ).toThrow();
  });

  it("should reject a payload with an unknown scene type", () => {
    expect(() =>
      parseCreatorScenesStats({
        address: "0xabc",
        as_of: "2026-07-21",
        scenes: [wireScene({ scene_type: "moon" })],
      }),
    ).toThrow();
  });
});

describe("when fetching creator scene stats", () => {
  beforeEach(() => {
    vi.mocked(signedGetJSON).mockReset();
  });

  it("should issue one signed GET against the creators-data mount", async () => {
    vi.mocked(signedGetJSON).mockResolvedValue({
      address: "0xabc",
      as_of: "2026-07-21",
      scenes: [],
    });
    const stats = await fetchCreatorScenesStats(identity);
    expect(signedGetJSON).toHaveBeenCalledTimes(1);
    expect(signedGetJSON).toHaveBeenCalledWith(SCENE_STATS_PATH, {
      identity,
      base: "https://decentraland.org/creators-data/api",
      signal: undefined,
    });
    expect(stats.scenes).toEqual([]);
  });

  it("should not double the /api segment", async () => {
    vi.mocked(signedGetJSON).mockResolvedValue({
      address: "0xabc",
      as_of: "2026-07-21",
      scenes: [],
    });
    await fetchCreatorScenesStats(identity, {
      base: "http://localhost:8787/api",
    });
    const call = vi.mocked(signedGetJSON).mock.calls[0];
    expect(`${(call[1] as { base: string }).base}${call[0]}`).toBe(
      "http://localhost:8787/api/creators/me/scenes/stats",
    );
    expect(`${(call[1] as { base: string }).base}${call[0]}`).not.toContain(
      "/api/api",
    );
  });
});
