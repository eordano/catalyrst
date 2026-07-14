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
  it("a world ABSENT from the snapshot is no-sample, never a live zero", () => {
    const { now } = joinWorldPresence(world(), presence([row("other.dcl.eth", 4)]));
    expect(now.state).toBe("no-sample");
    expect(Object.keys(now)).not.toContain("value");
    if (now.state !== "no-sample") throw new Error("unreachable");
    expect(now.note).toContain("Not the same as zero");
  });

  it("a world PRESENT with count 0 is a showable zero with the mandatory note", () => {
    const { now, note } = joinWorldPresence(
      world(),
      presence([row("petbarn.dcl.eth", 0)]),
    );
    expect(now.state).toBe("sampled");
    if (now.state !== "sampled") throw new Error("unreachable");
    expect(now.value).toBe(0);
    expect(note).toContain("a real zero");
  });

  it("a world present with a count is sampled at the row's own taken_at", () => {
    const { now, note } = joinWorldPresence(
      world(),
      presence([row("PETBARN.DCL.ETH", 2)]),
    );
    expect(now.state).toBe("sampled");
    if (now.state !== "sampled") throw new Error("unreachable");
    expect(now.value).toBe(2);
    expect(now.takenAt).toBe(TAKEN);
    expect(note).toBeNull();
  });

  it("a world with zero deployed scenes is unbuilt, not zero and not no-sample", () => {
    const { now } = joinWorldPresence(
      world({ deployedScenes: 0 }),
      presence([row("petbarn.dcl.eth", 3)]),
    );
    expect(now.state).toBe("unbuilt");
    if (now.state !== "unbuilt") throw new Error("unreachable");
    expect(now.reason).toContain("presence has never had anything to sample");
  });

  it("an UNKNOWN deployed-scene count does not take the never-deployed branch", () => {
    const { now } = joinWorldPresence(
      { name: "petbarn.dcl.eth", deployedScenes: null },
      presence([row("petbarn.dcl.eth", 2)]),
    );
    expect(now.state).toBe("sampled");
  });

  it("propagates a dead presence read instead of inventing a headcount", () => {
    const dead: Datum<WorldOccupancyRow[]> = {
      state: "unavailable",
      endpoint: ENDPOINT,
      status: 500,
      reason: "boom",
    };
    const { now } = joinWorldPresence(world(), dead);
    expect(now.state).toBe("unavailable");
  });
});

describe("joinLiveUsers", () => {
  const live: Datum<LiveData> = {
    state: "live",
    value: {
      data: { totalUsers: 5, perWorld: [{ worldName: "petbarn.dcl.eth", users: 3 }] },
      lastUpdated: TAKEN,
    },
    endpoint: "GET worlds-content-server.decentraland.org/live-data",
    readAt: TAKEN,
  };

  it("reads the world's own figure", () => {
    const { users, note } = joinLiveUsers("petbarn.dcl.eth", live);
    expect(users.state).toBe("live");
    if (users.state !== "live") throw new Error("unreachable");
    expect(users.value).toBe(3);
    expect(note).toBeNull();
  });

  it("treats an unlisted world as a real zero and says how that was derived", () => {
    const { users, note } = joinLiveUsers("elsewhere.dcl.eth", live);
    expect(users.state).toBe("live");
    if (users.state !== "live") throw new Error("unreachable");
    expect(users.value).toBe(0);
    expect(note).toContain("lists only rooms with users");
  });
});

describe("disagreeing sources are shown, never reconciled", () => {
  it("states the disagreement when the two differ", () => {
    expect(disagreementSentence(2, 3)).toContain("These disagree (2 vs 3)");
  });
  it("says nothing when they agree or when one side is missing", () => {
    expect(disagreementSentence(3, 3)).toBeNull();
    expect(disagreementSentence(null, 3)).toBeNull();
    expect(disagreementSentence(2, null)).toBeNull();
  });
});

describe("worldRowKind", () => {
  it("classifies deployed, never-deployed and blocked", () => {
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

// fails closed
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
  it("loadActivityIndex reports allUpstreamsDown and exposes no showable datum", async () => {
    const data = await loadActivityIndex({
      address: "0xabc",
      fetchImpl: rejectAll,
      wcsBase: "https://wcs.example.test",
    });
    expect(data.allUpstreamsDown).toBe(true);
    expect(data.rows).toEqual([]);
    expect(
      showableCount([
        data.worlds,
        data.presenceWorlds,
        data.presenceScenes,
        data.current,
        data.liveData,
      ]),
    ).toBe(0);
  });

  it("loadWorldActivity reports allUpstreamsDown and does not claim the world is unknown-for-sure", async () => {
    const data = await loadWorldActivity("petbarn.dcl.eth", {
      address: "0xabc",
      fetchImpl: rejectAll,
      wcsBase: "https://wcs.example.test",
    });
    expect(data.allUpstreamsDown).toBe(true);
    // Unreadable is not absent: with every upstream down nothing proved the
    // world missing, so the page must say could-not-read (503), never 404.
    expect(data.worldKnown).toBe(true);
    expect(
      showableCount([
        data.about,
        data.history,
        data.permissions,
        data.reception,
        data.realm,
        data.now,
        data.liveUsers,
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
    // presence is dead, so the headcount is unavailable -- not zero
    expect(data.rows[0].now.state).toBe("unavailable");
    // ...while the worlds server's own figure still shows
    expect(data.rows[0].liveUsers.state).toBe("live");
  });
});
