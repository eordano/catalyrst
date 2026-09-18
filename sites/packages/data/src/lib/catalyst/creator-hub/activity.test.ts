import { describe, expect, it } from "vitest";

import {
  disagreementSentence,
  parsePlacesWorlds,
  worldRowKind,
} from "./activity";
import {
  joinLiveUsers,
  joinWorldPresence,
  loadActivityIndex,
  loadWorldActivity,
} from "./activity.server";
import type { WorldOccupancyRow } from "../places/presence-history";
import type { Datum } from "./datum.server";
import type { LiveData } from "../wcs";
import type { ManagedWorld } from "./manage-worlds";

const TAKEN = "2026-08-01T09:50:32.602Z";
const ENDPOINT = "GET catalyst.example.com/presence/current/worlds";

function world(over: Partial<ManagedWorld> = {}): ManagedWorld {
  return {
    name: "petbarn.dcl.eth",
    owner: "0xabc",
    title: "Pet Barn",
    description: null,
    contentRating: null,
    spawnCoordinates: "0,0",
    lastDeployedAt: "2026-07-12T00:00:00.000Z",
    blockedSince: null,
    deployedScenes: 1,
    thumbnail: null,
    role: "owner",
    ...over,
  };
}

function presence(rows: WorldOccupancyRow[]): Datum<WorldOccupancyRow[]> {
  return {
    state: "sampled",
    value: rows,
    endpoint: ENDPOINT,
    readAt: TAKEN,
    takenAt: TAKEN,
    cadenceSeconds: 300,
  };
}

function row(name: string, count: number): WorldOccupancyRow {
  return { world_name: name, count, live_users: null, taken_at: TAKEN };
}

describe("the world -> presence join (the highest-risk rule in this build)", () => {
  it("absent from the snapshot is no-sample, present with 0 is a noted real zero, present with a count is sampled at the row's taken_at", () => {
    const absent = joinWorldPresence(world(), presence([row("other.dcl.eth", 4)])).now;
    expect(absent.state).toBe("no-sample");
    expect(Object.keys(absent)).not.toContain("value");
    if (absent.state !== "no-sample") throw new Error("unreachable");
    expect(absent.note).toContain("Not the same as zero");

    const zero = joinWorldPresence(world(), presence([row("petbarn.dcl.eth", 0)]));
    expect(zero.now.state).toBe("sampled");
    if (zero.now.state !== "sampled") throw new Error("unreachable");
    expect(zero.now.value).toBe(0);
    expect(zero.note).toContain("a real zero");

    const some = joinWorldPresence(world(), presence([row("PETBARN.DCL.ETH", 2)]));
    expect(some.now.state).toBe("sampled");
    if (some.now.state !== "sampled") throw new Error("unreachable");
    expect(some.now.value).toBe(2);
    expect(some.now.takenAt).toBe(TAKEN);
    expect(some.note).toBeNull();
  });

  it("zero deployed scenes is unbuilt, an unknown count is not, and a dead read propagates instead of inventing a headcount", () => {
    const unbuilt = joinWorldPresence(
      world({ deployedScenes: 0 }),
      presence([row("petbarn.dcl.eth", 3)]),
    ).now;
    expect(unbuilt.state).toBe("unbuilt");
    if (unbuilt.state !== "unbuilt") throw new Error("unreachable");
    expect(unbuilt.reason).toContain("presence has never had anything to sample");
    expect(
      joinWorldPresence(
        { name: "petbarn.dcl.eth", deployedScenes: null },
        presence([row("petbarn.dcl.eth", 2)]),
      ).now.state,
    ).toBe("sampled");
    const dead: Datum<WorldOccupancyRow[]> = {
      state: "unavailable",
      endpoint: ENDPOINT,
      status: 500,
      reason: "boom",
    };
    expect(joinWorldPresence(world(), dead).now.state).toBe("unavailable");
  });
});

describe("joinLiveUsers", () => {
  it("reads the world's own figure and treats an unlisted world as a derived real zero", () => {
    const live: Datum<LiveData> = {
      state: "live",
      value: {
        data: { totalUsers: 5, perWorld: [{ worldName: "petbarn.dcl.eth", users: 3 }] },
        lastUpdated: TAKEN,
      },
      endpoint: "GET worlds-content-server.decentraland.org/live-data",
      readAt: TAKEN,
    };
    const own = joinLiveUsers("petbarn.dcl.eth", live);
    expect(own.users.state).toBe("live");
    if (own.users.state !== "live") throw new Error("unreachable");
    expect(own.users.value).toBe(3);
    expect(own.note).toBeNull();
    const unlisted = joinLiveUsers("elsewhere.dcl.eth", live);
    expect(unlisted.users.state).toBe("live");
    if (unlisted.users.state !== "live") throw new Error("unreachable");
    expect(unlisted.users.value).toBe(0);
    expect(unlisted.note).toContain("lists only rooms with users");
  });
});

describe("row classification", () => {
  it("disagreeing sources are stated, never reconciled; worldRowKind classifies deployed, never-deployed and blocked", () => {
    expect(disagreementSentence(2, 3)).toContain("These disagree (2 vs 3)");
    expect(disagreementSentence(3, 3)).toBeNull();
    expect(disagreementSentence(null, 3)).toBeNull();
    expect(disagreementSentence(2, null)).toBeNull();
    expect(worldRowKind(world())).toBe("deployed");
    expect(worldRowKind(world({ deployedScenes: 0 }))).toBe("never-deployed");
    expect(worldRowKind(world({ blockedSince: "2026-01-01" }))).toBe("blocked");
  });
});

describe("places reception rows", () => {
  it("parses likes and favourites and ignores the known-bad visit fields", () => {
    const rows = parsePlacesWorlds({
      data: [
        {
          id: "world:041.dcl.eth",
          world_name: "041.dcl.eth",
          title: "Ultimate Game Party",
          likes: 4,
          dislikes: 1,
          favorites: 2,
          like_rate: 0.8,
          deployed_at: "2025-01-06T18:14:37.243Z",
          user_visits: 0,
          user_count: 0,
        },
      ],
      total: 1,
    });
    expect(rows).not.toBeNull();
    expect(rows).toHaveLength(1);
    expect(rows?.[0].likes).toBe(4);
    expect(rows?.[0]).not.toHaveProperty("user_visits");
    expect(rows?.[0]).not.toHaveProperty("user_count");
  });
});

const rejectAll = (async () => {
  throw new Error("network down");
}) as unknown as typeof fetch;

function showableCount(values: unknown[]): number {
  return values.filter(
    (d) =>
      typeof d === "object" &&
      d !== null &&
      "value" in (d as Record<string, unknown>),
  ).length;
}

describe("every upstream down", () => {
  it("both loaders report allUpstreamsDown, expose no showable datum, and never claim the world is unknown-for-sure", async () => {
    const index = await loadActivityIndex({
      address: "0xabc",
      fetchImpl: rejectAll,
      wcsBase: "https://wcs.example.test",
    });
    expect(index.allUpstreamsDown).toBe(true);
    expect(index.rows).toEqual([]);
    expect(
      showableCount([
        index.worlds,
        index.presenceWorlds,
        index.presenceScenes,
        index.current,
        index.liveData,
      ]),
    ).toBe(0);

    const detail = await loadWorldActivity("petbarn.dcl.eth", {
      address: "0xabc",
      fetchImpl: rejectAll,
      wcsBase: "https://wcs.example.test",
    });
    expect(detail.allUpstreamsDown).toBe(true);
    expect(detail.worldKnown).toBe(true);
    expect(
      showableCount([
        detail.about,
        detail.history,
        detail.permissions,
        detail.reception,
        detail.realm,
        detail.now,
        detail.liveUsers,
      ]),
    ).toBe(0);
  });
});

describe("partial degradation stays partial", () => {
  it("presence down + wcs up still returns showable wcs data", async () => {
    const fetchImpl = (async (url: string) => {
      if (url.includes("/presence/")) throw new Error("presence down");
      if (url.includes("wcs.example.test/worlds")) {
        return new Response(
          JSON.stringify({
            total: 1,
            worlds: [
              {
                name: "petbarn.dcl.eth",
                owner: "0xabc",
                title: "Pet Barn",
                description: null,
                content_rating: null,
                spawn_coordinates: "0,0",
                last_deployed_at: "2026-07-12T00:00:00.000Z",
                blocked_since: null,
                deployed_scenes: 1,
                thumbnail_hash: null,
              },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/live-data")) {
        return new Response(
          JSON.stringify({
            data: { totalUsers: 3, perWorld: [{ worldName: "petbarn.dcl.eth", users: 3 }] },
            lastUpdated: TAKEN,
          }),
          { status: 200 },
        );
      }
      throw new Error("not stubbed");
    }) as unknown as typeof fetch;

    const data = await loadActivityIndex({
      address: "0xabc",
      fetchImpl,
      wcsBase: "https://wcs.example.test",
    });

    expect(data.allUpstreamsDown).toBe(false);
    expect(data.worlds.state).toBe("live");
    expect(data.rows).toHaveLength(1);
    expect(data.rows[0].now.state).toBe("unavailable");
    expect(data.rows[0].liveUsers.state).toBe("live");
  });
});
