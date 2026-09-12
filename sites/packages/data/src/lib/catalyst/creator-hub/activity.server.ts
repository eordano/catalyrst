import { CatalystError, buildQuery, catalystBase, getJSON } from "../client";
import type { GetOptions, Query } from "../client";
import { placesApiPath } from "../typed";
import {
  CurrentSnapshotSchema,
  worldJumpUrl,
  type CurrentSnapshot,
} from "../places/presence";
import {
  currentPath,
  currentScenesPath,
  currentWorldsPath,
  fetchCurrentSceneRows,
  fetchCurrentWorldRows,
  type BucketizedHistory,
  type SceneHistoryRow,
  type WorldOccupancyRow,
} from "../places/presence-history";
import {
  loadSceneOccupancyHistory,
  loadWorldOccupancyHistory,
} from "../places/presence-history.server";
import { loadLiveData, loadMyWorlds, type MyWorlds } from "../wcs.server";
import { liveUsersFor, type LiveData } from "../wcs";
import {
  loadWorldPermissions,
  type LoadWorldPermissionsResult,
} from "./world-permissions.server";
import type { ManagedWorld } from "./manage-worlds";
import {
  HISTORY_LIMIT_PEAK_7D,
  HISTORY_LIMIT_WORLD_PAGE,
  LIVE_DATA_ZERO_NOTE,
  NEVER_DEPLOYED_REASON,
  NEVER_DEPLOYED_TODAY,
  PlacesWorldRowSchema,
  RealmAboutSchema,
  WorldAboutSchema,
  disagreementSentence,
  noSampleNote,
  parsePlacesWorlds,
  realZeroNote,
  worldRowKind,
  type PlacesWorldRow,
  type RealmAbout,
  type WorldAbout,
  type WorldRowKind,
} from "./activity";
import {
  DEFAULT_CADENCE_SECONDS,
  endpointLabel,
  liveNow,
  noSample,
  sampleTime,
  sampledAt,
  showable,
  unavailableFrom,
  unbuiltDatum,
  type Datum,
} from "./datum.server";

export type ActivityOptions = {
  address?: string | null;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
  wcsBase?: string;
};

export const PEAK_LOOKUP_LIMIT = 50;

function label(path: string, query?: Query): string {
  return endpointLabel("GET", `${catalystBase()}${path}${buildQuery(query)}`);
}

function get(opts: ActivityOptions): GetOptions {
  return { signal: opts.signal, fetchImpl: opts.fetchImpl };
}

async function settled<T>(p: Promise<T>): Promise<{ ok: true; value: T } | { ok: false; err: unknown }> {
  try {
    return { ok: true, value: await p };
  } catch (err) {
    return { ok: false, err };
  }
}

async function loadCurrentSnapshot(
  opts: ActivityOptions,
): Promise<Datum<CurrentSnapshot>> {
  const endpoint = label(currentPath());
  const res = await settled(getJSON<unknown>(currentPath(), get(opts)));
  if (!res.ok) return unavailableFrom(res.err, endpoint);
  const env = (res.value as { current?: unknown })?.current;
  const parsed = CurrentSnapshotSchema.safeParse(env);
  if (!parsed.success) {
    return unavailableFrom(
      new Error("unexpected payload shape"),
      endpoint,
      "The snapshot header did not match the presence shape.",
    );
  }
  return sampledAt(
    parsed.data,
    endpoint,
    parsed.data.taken_at,
    DEFAULT_CADENCE_SECONDS,
  );
}

async function loadCurrentWorlds(
  opts: ActivityOptions,
): Promise<Datum<WorldOccupancyRow[]>> {
  const endpoint = label(currentWorldsPath());
  const res = await settled(fetchCurrentWorldRows(get(opts)));
  if (!res.ok) return unavailableFrom(res.err, endpoint);
  const takenAt = res.value[0]?.taken_at ?? "";
  if (!takenAt) {
    return noSample(
      endpoint,
      new Date().toISOString(),
      "The last presence snapshot listed no live worlds at all.",
    );
  }
  return sampledAt(res.value, endpoint, takenAt, DEFAULT_CADENCE_SECONDS);
}

async function loadCurrentScenes(
  opts: ActivityOptions,
): Promise<Datum<SceneHistoryRow[]>> {
  const endpoint = label(currentScenesPath());
  const res = await settled(fetchCurrentSceneRows(get(opts)));
  if (!res.ok) return unavailableFrom(res.err, endpoint);
  const takenAt = res.value[0]?.taken_at ?? "";
  if (!takenAt) {
    return noSample(
      endpoint,
      new Date().toISOString(),
      "The last presence snapshot listed no occupied scenes at all.",
    );
  }
  return sampledAt(res.value, endpoint, takenAt, DEFAULT_CADENCE_SECONDS);
}

export type JoinSubject = { name: string; deployedScenes: number | null };

export function joinWorldPresence(
  world: JoinSubject,
  rows: Datum<WorldOccupancyRow[]>,
  cadenceSeconds = DEFAULT_CADENCE_SECONDS,
): { now: Datum<number>; note: string | null } {
  if (world.deployedScenes !== null && world.deployedScenes <= 0) {
    return {
      now: unbuiltDatum(
        `occupancy for ${world.name}`,
        NEVER_DEPLOYED_REASON,
        NEVER_DEPLOYED_TODAY,
      ),
      note: null,
    };
  }
  if (!showable(rows)) return { now: rows as Datum<number>, note: null };

  const key = world.name.trim().toLowerCase();
  const hit = rows.value.find((r) => r.world_name.trim().toLowerCase() === key);
  const takenAt = hit?.taken_at || rows.value[0]?.taken_at || sampleTime(rows);

  if (!hit) {
    return {
      now: noSample(rows.endpoint, takenAt, noSampleNote(takenAt)),
      note: null,
    };
  }
  return {
    now: sampledAt(hit.count, rows.endpoint, takenAt, cadenceSeconds),
    note: hit.count === 0 ? realZeroNote(takenAt) : null,
  };
}

export function joinLiveUsers(
  world: string,
  live: Datum<LiveData>,
): { users: Datum<number>; note: string | null } {
  if (!showable(live)) return { users: live as Datum<number>, note: null };
  const users = liveUsersFor(live.value, world);
  if (users === null) {
    return { users: liveNow(0, live.endpoint), note: LIVE_DATA_ZERO_NOTE };
  }
  return { users: liveNow(users, live.endpoint), note: null };
}

export type ActivityWorldRow = {
  world: ManagedWorld;
  kind: WorldRowKind;
  jumpUrl: string;
  now: Datum<number>;
  nowNote: string | null;
  liveUsers: Datum<number>;
  liveUsersNote: string | null;
  disagreement: string | null;
  peak7d: Datum<number>;
};

export type ActivityIndexData = {
  address: string;
  readAt: string;
  worlds: Datum<MyWorlds>;
  rows: ActivityWorldRow[];
  current: Datum<CurrentSnapshot>;
  presenceWorlds: Datum<WorldOccupancyRow[]>;
  presenceScenes: Datum<SceneHistoryRow[]>;
  liveData: Datum<LiveData>;
  allUpstreamsDown: boolean;
};

function peakDatum(history: Datum<BucketizedHistory>): Datum<number> {
  if (!showable(history)) return history as Datum<number>;
  const takenAt = sampleTime(history);
  if (history.value.peak === null) {
    return noSample(
      history.endpoint,
      takenAt,
      "No snapshots in the last 7 days, so there is no peak to report. Not the same as a peak of zero.",
    );
  }
  return sampledAt(
    history.value.peak,
    history.endpoint,
    takenAt,
    history.value.cadenceSeconds || DEFAULT_CADENCE_SECONDS,
  );
}

async function peakFor(
  world: ManagedWorld,
  index: number,
  opts: ActivityOptions,
): Promise<Datum<number>> {
  if (world.deployedScenes <= 0) {
    return unbuiltDatum(
      `7-day peak for ${world.name}`,
      NEVER_DEPLOYED_REASON,
      NEVER_DEPLOYED_TODAY,
    );
  }
  if (index >= PEAK_LOOKUP_LIMIT) {
    return noSample(
      label("/presence/worlds/history", {
        world: world.name,
        limit: HISTORY_LIMIT_PEAK_7D,
      }),
      new Date().toISOString(),
      `Not requested: this page reads 7-day peaks for the first ${PEAK_LOOKUP_LIMIT} worlds only. Open the world to read its history.`,
    );
  }
  const history = await loadWorldOccupancyHistory(
    world.name,
    HISTORY_LIMIT_PEAK_7D,
    get(opts),
  );
  return peakDatum(history);
}

function isUnavailable(d: Datum<unknown>): boolean {
  return d.state === "unavailable";
}

async function settleAll(
  entries: { endpoint: string; load: () => Promise<Datum<unknown>> }[],
): Promise<Datum<unknown>[]> {
  const results = await Promise.allSettled(entries.map((e) => e.load()));
  return results.map((r, i) =>
    r.status === "fulfilled"
      ? r.value
      : unavailableFrom(r.reason, entries[i].endpoint),
  );
}

const NO_ADDRESS_NOTE =
  "No address yet. This page needs an address to pick which worlds to show. It is not a login \u{2014} the data is public either way.";

const WCS_WORLDS_ENDPOINT =
  "GET worlds-content-server.decentraland.org/worlds?authorized_deployer=";

export async function loadActivityIndex(
  opts: ActivityOptions = {},
): Promise<ActivityIndexData> {
  const address = (opts.address ?? "").trim().toLowerCase();
  const readAt = new Date().toISOString();

  let aboutAbsent = false;
  const settledResults = await settleAll([
    {
      endpoint: WCS_WORLDS_ENDPOINT,
      load: () =>
        address
          ? loadMyWorlds(address, {
              base: opts.wcsBase,
              signal: opts.signal,
              fetchImpl: opts.fetchImpl,
            })
          : Promise.resolve(
              noSample(WCS_WORLDS_ENDPOINT, readAt, NO_ADDRESS_NOTE),
            ),
    },
    { endpoint: label(currentWorldsPath()), load: () => loadCurrentWorlds(opts) },
    { endpoint: label(currentPath()), load: () => loadCurrentSnapshot(opts) },
    { endpoint: label(currentScenesPath()), load: () => loadCurrentScenes(opts) },
    {
      endpoint: "GET worlds-content-server.decentraland.org/live-data",
      load: () =>
        loadLiveData({
          base: opts.wcsBase,
          signal: opts.signal,
          fetchImpl: opts.fetchImpl,
        }),
    },
  ]);
  const worlds = settledResults[0] as Datum<MyWorlds>;
  const presenceWorlds = settledResults[1] as Datum<WorldOccupancyRow[]>;
  const current = settledResults[2] as Datum<CurrentSnapshot>;
  const presenceScenes = settledResults[3] as Datum<SceneHistoryRow[]>;
  const liveData = settledResults[4] as Datum<LiveData>;

  const list = showable(worlds) ? worlds.value.worlds : [];
  const settledRows = await Promise.allSettled(
    list.map(async (world, index): Promise<ActivityWorldRow> => {
      const { now, note } = joinWorldPresence(world, presenceWorlds);
      const { users, note: liveNote } = joinLiveUsers(world.name, liveData);
      return {
        world,
        kind: worldRowKind(world),
        jumpUrl: worldJumpUrl(world.name),
        now,
        nowNote: note,
        liveUsers: users,
        liveUsersNote: liveNote,
        disagreement: disagreementSentence(
          showable(now) ? now.value : null,
          showable(users) ? users.value : null,
        ),
        peak7d: await peakFor(world, index, opts),
      };
    }),
  );
  const rows: ActivityWorldRow[] = settledRows.flatMap((r) =>
    r.status === "fulfilled" ? [r.value] : [],
  );

  return {
    address,
    readAt,
    worlds,
    rows,
    current,
    presenceWorlds,
    presenceScenes,
    liveData,
    allUpstreamsDown:
      isUnavailable(worlds) &&
      isUnavailable(presenceWorlds) &&
      isUnavailable(current) &&
      isUnavailable(presenceScenes) &&
      isUnavailable(liveData),
  };
}

export type WorldActivityData = {
  world: string;
  address: string;
  readAt: string;
  row: ManagedWorld | null;
  worldKnown: boolean;
  foreign: boolean;
  about: Datum<WorldAbout>;
  realm: Datum<RealmAbout>;
  now: Datum<number>;
  nowNote: string | null;
  liveUsers: Datum<number>;
  liveUsersNote: string | null;
  disagreement: string | null;
  history: Datum<BucketizedHistory>;
  permissions: Datum<LoadWorldPermissionsResult["permissions"]>;
  reception: Datum<PlacesWorldRow[]>;
  myWorlds: Datum<MyWorlds>;
  allUpstreamsDown: boolean;
};

async function loadWorldAbout(
  world: string,
  opts: ActivityOptions,
): Promise<{ datum: Datum<WorldAbout>; absent: boolean }> {
  const path = `/world/${encodeURIComponent(world)}/about`;
  const endpoint = label(path);
  const res = await settled(getJSON<unknown>(path, get(opts)));
  if (!res.ok) {
    const absent =
      res.err instanceof CatalystError && res.err.status === 404;
    return {
      absent,
      datum: unavailableFrom(
        res.err,
        endpoint,
        "catalyst answers 404 here for a world with no scenes deployed as well as for a world it has never heard of.",
      ),
    };
  }
  const parsed = WorldAboutSchema.safeParse(res.value);
  if (!parsed.success) {
    return {
      absent: false,
      datum: unavailableFrom(new Error("unexpected payload shape"), endpoint),
    };
  }
  return { absent: false, datum: liveNow(parsed.data, endpoint) };
}

async function loadRealmAbout(opts: ActivityOptions): Promise<Datum<RealmAbout>> {
  const endpoint = label("/about");
  const res = await settled(getJSON<unknown>("/about", get(opts)));
  if (!res.ok) return unavailableFrom(res.err, endpoint);
  const parsed = RealmAboutSchema.safeParse(res.value);
  if (!parsed.success) {
    return unavailableFrom(new Error("unexpected payload shape"), endpoint);
  }
  return liveNow(parsed.data, endpoint);
}

async function loadReception(
  world: string,
  opts: ActivityOptions,
): Promise<Datum<PlacesWorldRow[]>> {
  const path = placesApiPath("get", "/api/worlds");
  const query: Query = { names: world };
  const endpoint = label(path, query);
  const res = await settled(getJSON<unknown>(path, { ...get(opts), query }));
  if (!res.ok) return unavailableFrom(res.err, endpoint);
  const rows = parsePlacesWorlds(res.value);
  if (rows === null) {
    return unavailableFrom(new Error("unexpected payload shape"), endpoint);
  }
  return liveNow(rows, endpoint);
}

async function loadPermissionsDatum(
  world: string,
  opts: ActivityOptions,
): Promise<Datum<LoadWorldPermissionsResult["permissions"]>> {
  const endpoint = endpointLabel(
    "GET",
    `${catalystBase()}/world/${encodeURIComponent(world)}/permissions`,
  );
  const res = await settled(
    loadWorldPermissions(world, { ...get(opts), base: catalystBase() }),
  );
  if (!res.ok) return unavailableFrom(res.err, endpoint);
  if (res.value.fallback) {
    return unavailableFrom(
      new Error("permissions read did not return a usable payload"),
      endpoint,
      "An unread ACL and an empty ACL are not the same thing, so no list is shown.",
    );
  }
  return liveNow(res.value.permissions, endpoint);
}

export async function loadWorldActivity(
  world: string,
  opts: ActivityOptions = {},
): Promise<WorldActivityData> {
  const address = (opts.address ?? "").trim().toLowerCase();
  const readAt = new Date().toISOString();

  let aboutAbsent = false;
  const settledResults = await settleAll([
    {
      endpoint: WCS_WORLDS_ENDPOINT,
      load: () =>
        address
          ? loadMyWorlds(address, {
              base: opts.wcsBase,
              signal: opts.signal,
              fetchImpl: opts.fetchImpl,
            })
          : Promise.resolve(
              noSample(
                WCS_WORLDS_ENDPOINT,
                readAt,
                "No address supplied, so this page cannot say whether you deploy this world. The occupancy below is public either way.",
              ),
            ),
    },
    { endpoint: label(currentWorldsPath()), load: () => loadCurrentWorlds(opts) },
    {
      endpoint: "GET worlds-content-server.decentraland.org/live-data",
      load: () =>
        loadLiveData({
          base: opts.wcsBase,
          signal: opts.signal,
          fetchImpl: opts.fetchImpl,
        }),
    },
    {
      endpoint: label("/presence/worlds/history", {
        world,
        limit: HISTORY_LIMIT_WORLD_PAGE,
      }),
      load: () =>
        loadWorldOccupancyHistory(world, HISTORY_LIMIT_WORLD_PAGE, get(opts)),
    },
    {
      endpoint: label(`/world/${encodeURIComponent(world)}/about`),
      load: async () => {
        const r = await loadWorldAbout(world, opts);
        aboutAbsent = r.absent;
        return r.datum;
      },
    },
    {
      endpoint: label(`/world/${encodeURIComponent(world)}/permissions`),
      load: () => loadPermissionsDatum(world, opts),
    },
    {
      endpoint: label(placesApiPath("get", "/api/worlds"), { names: world }),
      load: () => loadReception(world, opts),
    },
    { endpoint: label("/about"), load: () => loadRealmAbout(opts) },
  ]);
  const myWorlds = settledResults[0] as Datum<MyWorlds>;
  const presenceWorlds = settledResults[1] as Datum<WorldOccupancyRow[]>;
  const liveData = settledResults[2] as Datum<LiveData>;
  const history = settledResults[3] as Datum<BucketizedHistory>;
  const about = settledResults[4] as Datum<WorldAbout>;
  const permissions = settledResults[5] as Datum<
    LoadWorldPermissionsResult["permissions"]
  >;
  const reception = settledResults[6] as Datum<PlacesWorldRow[]>;
  const realm = settledResults[7] as Datum<RealmAbout>;

  const key = world.trim().toLowerCase();
  const row = showable(myWorlds)
    ? (myWorlds.value.worlds.find((w) => w.name.trim().toLowerCase() === key) ?? null)
    : null;

  const inPresence =
    showable(presenceWorlds) &&
    presenceWorlds.value.some((r) => r.world_name.trim().toLowerCase() === key);
  const worldKnown =
    row !== null || showable(about) || inPresence || !aboutAbsent;

  const subject: JoinSubject = {
    name: world,
    deployedScenes: row ? row.deployedScenes : null,
  };

  const { now, note } = joinWorldPresence(subject, presenceWorlds);
  const { users, note: liveNote } = joinLiveUsers(world, liveData);

  return {
    world,
    address,
    readAt,
    row,
    worldKnown,
    foreign: row === null,
    about,
    realm,
    now,
    nowNote: note,
    liveUsers: users,
    liveUsersNote: liveNote,
    disagreement: disagreementSentence(
      showable(now) ? now.value : null,
      showable(users) ? users.value : null,
    ),
    history,
    permissions,
    reception,
    myWorlds,
    allUpstreamsDown:
      isUnavailable(presenceWorlds) &&
      isUnavailable(liveData) &&
      isUnavailable(history) &&
      isUnavailable(about) &&
      isUnavailable(permissions) &&
      isUnavailable(reception) &&
      isUnavailable(realm),
  };
}

export type SceneActivityData = {
  pointer: string;
  readAt: string;
  now: Datum<number>;
  nowNote: string | null;
  sceneName: string | null;
  history: Datum<BucketizedHistory>;
};

export async function loadSceneActivity(
  pointer: string,
  opts: ActivityOptions = {},
): Promise<SceneActivityData> {
  const readAt = new Date().toISOString();
  const [scenes, history] = await Promise.all([
    loadCurrentScenes(opts),
    loadSceneOccupancyHistory(pointer, HISTORY_LIMIT_WORLD_PAGE, get(opts)),
  ]);

  const key = pointer.trim();
  let now: Datum<number>;
  let nowNote: string | null = null;
  let sceneName: string | null = null;

  if (!showable(scenes)) {
    now = scenes as Datum<number>;
  } else {
    const hit = scenes.value.find((r) => r.pointer.trim() === key);
    const takenAt =
      hit?.taken_at || scenes.value[0]?.taken_at || sampleTime(scenes);
    if (!hit) {
      now = noSample(
        scenes.endpoint,
        takenAt,
        `No sample: ${pointer} was not occupied at the last snapshot (${takenAt}). Not the same as zero.`,
      );
    } else {
      sceneName = hit.scene_name;
      now = sampledAt(hit.count, scenes.endpoint, takenAt, DEFAULT_CADENCE_SECONDS);
      if (hit.count === 0) nowNote = realZeroNote(takenAt);
    }
  }

  return { pointer, readAt, now, nowNote, sceneName, history };
}
