import path from "node:path";

import { describe, expect, it } from "vitest";

import { parseStory } from "@core/lib/experiments/context";
import {
  CurrentSnapshotSchema,
  SceneOccupancyRowSchema,
  WorldHeadcountRowSchema,
  occupancyTotals,
  parsePointer,
  sceneJumpUrl,
  worldJumpUrl,
  type CurrentSnapshot,
  type PresenceSnapshot,
} from "./presence";

describe("presence schemas (generated, wire-strict)", () => {
  it("parses a current snapshot (null worlds_live_total tolerated), a full scene row (only scene_name nullable), and a world headcount row", () => {
    const s = CurrentSnapshotSchema.parse({
      snapshot_id: 1,
      taken_at: "2026-06-24T00:00:00Z",
      peers_count: 36,
      islands_count: 6,
      hot_scenes_count: 12,
      scenes_polled: 12,
      scene_users_total: 30,
      worlds_polled: 3,
      active_worlds: 3,
      world_users_total: 6,
      worlds_live_total: null,
    });
    expect(s.peers_count).toBe(36);
    expect(s.worlds_live_total).toBeNull();

    const row = SceneOccupancyRowSchema.parse({
      taken_at: "2026-06-24T00:00:00Z",
      pointer: "-3,-2",
      scene_name: null,
      realm: "main",
      count: 8,
    });
    expect(row.scene_name).toBeNull();
    expect(row.realm).toBe("main");

    const w = WorldHeadcountRowSchema.parse({
      taken_at: "2026-06-24T00:00:00Z",
      world_name: "museum.dcl.eth",
      count: 2,
      live_users: 2,
    });
    expect(w.count).toBe(2);
  });

  it("rejects rows that are missing wire-required fields", () => {
    expect(
      SceneOccupancyRowSchema.safeParse({
        taken_at: "2026-06-24T00:00:00Z",
        pointer: "-3,-2",
        scene_name: null,
        count: 8,
      }).success,
    ).toBe(false);
    expect(SceneOccupancyRowSchema.safeParse({ pointer: "-3,-2", count: 8 }).success).toBe(
      false,
    );
    expect(WorldHeadcountRowSchema.safeParse({ world_name: "a.dcl.eth" }).success).toBe(
      false,
    );
    expect(CurrentSnapshotSchema.safeParse({ snapshot_id: 1 }).success).toBe(false);
  });
});

describe("parsePointer + jump urls", () => {
  it("parses x,y tolerating junk and builds genesis + world jump links", () => {
    expect(parsePointer("144,-7")).toEqual([144, -7]);
    expect(parsePointer("")).toEqual([0, 0]);
    expect(sceneJumpUrl("-3,-2")).toBe(
      "https://catalyst.example.com/play/?position=-3,-2",
    );
    expect(worldJumpUrl("a b.dcl.eth")).toBe(
      "https://catalyst.example.com/play/?realm=a%20b.dcl.eth",
    );
  });
});

function zeroedCurrent(): CurrentSnapshot {
  return {
    snapshot_id: 0,
    taken_at: "2026-06-24T00:00:00Z",
    peers_count: 0,
    islands_count: 0,
    hot_scenes_count: 0,
    scenes_polled: 0,
    scene_users_total: 0,
    worlds_polled: 0,
    active_worlds: 0,
    world_users_total: 0,
    worlds_live_total: null,
  };
}

describe("occupancyTotals", () => {
  it("is null when any of the three reads is missing, and still reports empty when nobody is anywhere", () => {
    expect(
      occupancyTotals({
        current: null,
        scenes: null,
        worlds: null,
        source: "unavailable",
      }),
    ).toBeNull();
    expect(
      occupancyTotals({
        current: zeroedCurrent(),
        scenes: [],
        worlds: null,
        source: "unavailable",
      }),
    ).toBeNull();

    const idle: PresenceSnapshot = {
      current: zeroedCurrent(),
      scenes: [],
      worlds: [
        { taken_at: "2026-06-24T00:00:00Z", world_name: "idle.dcl.eth", count: 0, live_users: 0 },
      ],
      source: "catalyst",
    };
    const t = occupancyTotals(idle);
    expect(t?.peers).toBe(0);
    expect(t?.activeScenes).toBe(0);
    expect(t?.activeWorlds).toBe(0);
  });

  it("derives headline numbers preferring summary counts, and folds per-world live_users when the comms summary reports zero (worlds steady state)", () => {
    const snap: PresenceSnapshot = {
      current: {
        ...zeroedCurrent(),
        peers_count: 36,
        scenes_polled: 12,
        worlds_polled: 3,
        scene_users_total: 30,
        world_users_total: 6,
      },
      scenes: [
        { taken_at: "2026-06-24T00:00:00Z", pointer: "-3,-2", scene_name: "Plaza", realm: "main", count: 8 },
        { taken_at: "2026-06-24T00:00:00Z", pointer: "0,0", scene_name: null, realm: "main", count: 0 },
      ],
      worlds: [
        { taken_at: "2026-06-24T00:00:00Z", world_name: "museum.dcl.eth", count: 2, live_users: 2 },
      ],
      source: "catalyst",
    };
    const t = occupancyTotals(snap);
    expect(t?.peers).toBe(36);
    expect(t?.scenes).toBe(12);
    expect(t?.worlds).toBe(3);
    expect(t?.activeScenes).toBe(1);
    expect(t?.activeWorlds).toBe(1);

    const steady: PresenceSnapshot = {
      current: { ...zeroedCurrent(), worlds_polled: 4, worlds_live_total: 9 },
      scenes: [],
      worlds: [
        { taken_at: "2026-06-24T00:00:00Z", world_name: "kickoff.dcl.eth", count: 0, live_users: 1 },
        { taken_at: "2026-06-24T00:00:00Z", world_name: "pokernight.dcl.eth", count: 0, live_users: 5 },
      ],
      source: "catalyst",
    };
    const folded = occupancyTotals(steady);
    expect(folded?.activeWorlds).toBe(2);
    expect(folded?.worldUsers).toBe(9);
    expect(folded?.peers).toBe(9);
    expect(folded?.activeScenes).toBe(0);
  });
});

describe("operator-metrics story.md", () => {
  it("parses with valid frontmatter (single-arm dashboard experiment)", () => {
    const story = parseStory(
      path.join(process.cwd(), "packages", "features", "src", "stories", "admin/operator-metrics"),
    );
    expect(story.id).toBe("operator-metrics");
    expect(story.experiment.variants.map((v) => v.id)).toContain("dashboard");
  });
});
