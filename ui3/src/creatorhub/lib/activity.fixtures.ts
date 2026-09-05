/**
 * Story fixtures for the Activity and Data-sources screens.
 *
 * **Story data, not page data.** No component in this library imports this
 * module and no default prop references it. A page with no data renders an
 * unavailable state; it never falls back to anything in here. The numbers below
 * exist so Storybook can render each state deterministically, and they are
 * shaped after real readings so the layouts are honest about size.
 */

import type {
  SourceLedgerGroup,
  SourceLedgerRow,
} from "../components/SourceLedger";
import type {
  ActivityWorldRow,
  BusiestRow,
  ParcelActivity,
} from "../pages/ChActivityIndexPage";
import type {
  FactRow,
  NotBuiltSpec,
  StorageReading,
  WorldHistory,
  WorldMeta,
} from "../pages/ChWorldActivityPage";
import {
  DEFAULT_CADENCE_SECONDS,
  live,
  noSample,
  sampled,
  unavailable,
  unbuilt,
  type Datum,
} from "./datum";
import { FIXTURE_NOW, at } from "./datum.fixtures";
import type { ChartSeries, DailyPoint } from "./scene-analytics";

export { FIXTURE_NOW, at };

const READ_AT = at(0);
const TAKEN_AT = at(2 * 60_000);

export const WCS = "worlds-content-server.decentraland.org";
export const CATALYST = "catalyst.example.com";

const E = {
  worlds: `GET ${WCS}/worlds?authorized_deployer=`,
  walletStats: `GET ${WCS}/wallet/{address}/stats`,
  liveData: `GET ${WCS}/live-data`,
  status: `GET ${WCS}/status`,
  presenceCurrent: `GET ${CATALYST}/presence/current`,
  presenceWorlds: `GET ${CATALYST}/presence/current/worlds`,
  presenceScenes: `GET ${CATALYST}/presence/current/scenes`,
  worldHistory: `GET ${CATALYST}/presence/worlds/history?world=`,
  sceneHistory: `GET ${CATALYST}/presence/scenes/history?pointer=`,
  about: `GET ${CATALYST}/about`,
  worldAbout: `GET ${CATALYST}/world/{name}/about`,
  worldPermissions: `GET ${CATALYST}/world/{name}/permissions`,
  names: `GET ${CATALYST}/lambdas/users/{address}/names`,
  places: "GET places.decentraland.org/api/worlds?names=",
  kvWorld: `GET ${CATALYST}/world-storage/usage/world`,
  creatorsStats: "GET creators-data/creators/me/scenes/stats",
  creatorsMetrics: "GET creators-data/api/worlds/{world}/metrics",
} as const;

const s = <T,>(value: T, endpoint: string): Datum<T> =>
  sampled(value, endpoint, TAKEN_AT, DEFAULT_CADENCE_SECONDS, READ_AT);

const l = <T,>(value: T, endpoint: string): Datum<T> =>
  live(value, endpoint, READ_AT);

export const activityWorldRows: ActivityWorldRow[] = [
  {
    name: "petbarn.dcl.eth",
    title: "Pet Barn",
    lastDeployedAt: "2026-07-12T09:41:00Z",
    deployedScenes: 1,
    blockedSince: null,
    now: s(2, E.presenceWorlds),
    peak7d: s(6, `${E.worldHistory}petbarn.dcl.eth&limit=2016`),
    href: "/creator-hub/activity/petbarn.dcl.eth",
    jumpUrl: "https://decentraland.org/jump/?realm=petbarn.dcl.eth",
  },
  {
    name: "quietgallery.dcl.eth",
    title: "Quiet Gallery",
    lastDeployedAt: "2026-06-30T18:02:00Z",
    deployedScenes: 1,
    blockedSince: null,
    // Present in the snapshot with a count of zero: a real, sampled zero.
    now: s(0, E.presenceWorlds),
    peak7d: s(1, `${E.worldHistory}quietgallery.dcl.eth&limit=2016`),
    href: "/creator-hub/activity/quietgallery.dcl.eth",
  },
  {
    name: "offpoll.dcl.eth",
    title: null,
    lastDeployedAt: "2026-05-04T11:20:00Z",
    deployedScenes: 2,
    blockedSince: null,
    // Absent from the snapshot entirely: not the same fact as a zero.
    now: noSample(
      E.presenceWorlds,
      TAKEN_AT,
      "no sample: this world was not live at the last snapshot. Not the same as zero.",
    ),
    peak7d: noSample(
      `${E.worldHistory}offpoll.dcl.eth&limit=2016`,
      TAKEN_AT,
      "no sample: the collector has never polled this world.",
    ),
    href: "/creator-hub/activity/offpoll.dcl.eth",
  },
  {
    name: "unclaimed.dcl.eth",
    title: null,
    lastDeployedAt: null,
    deployedScenes: 0,
    blockedSince: null,
    now: noSample(
      E.presenceWorlds,
      TAKEN_AT,
      "nothing has ever been deployed to this NAME.",
    ),
    peak7d: noSample(
      E.presenceWorlds,
      TAKEN_AT,
      "nothing has ever been deployed to this NAME.",
    ),
    href: "/creator-hub/activity/unclaimed.dcl.eth",
    publishHref: "/creator-hub/deploy-world?name=unclaimed.dcl.eth",
  },
  {
    name: "onhold.dcl.eth",
    title: "On Hold",
    lastDeployedAt: "2026-02-11T07:00:00Z",
    deployedScenes: 1,
    blockedSince: "2026-06-18T00:00:00Z",
    now: noSample(E.presenceWorlds, TAKEN_AT, "blocked worlds are not polled."),
    peak7d: noSample(
      E.presenceWorlds,
      TAKEN_AT,
      "blocked worlds are not polled.",
    ),
    href: "/creator-hub/activity/onhold.dcl.eth",
  },
];

export const busiestScenes: BusiestRow[] = [
  { key: "-3,-2", label: "Plaza Corner", sub: "-3,-2", count: 4 },
  { key: "12,-40", label: "Sandbox", sub: "12,-40", count: 1 },
];

export const busiestWorlds: BusiestRow[] = [
  { key: "petbarn.dcl.eth", label: "petbarn.dcl.eth", count: 2 },
  { key: "kickoff.dcl.eth", label: "kickoff.dcl.eth", count: 1 },
];

function occupancyPoints(): DailyPoint[] {
  // A short, deliberately gappy series: 06:00-06:35 sampled, a 40-minute hole
  // where the collector produced nothing, then samples again. Nulls, not zeros.
  const values: (number | null)[] = [
    1, 2, 2, 3, 2, 1, 1, null, null, null, null, null, null, null, 2, 3, 2, 1,
  ];
  const base = Date.parse("2026-07-31T06:00:00Z");
  return values.map((value, i) => ({
    date: new Date(base + i * 300_000).toISOString(),
    value,
  }));
}

export const occupancySeries: ChartSeries[] = [
  {
    key: "headcount",
    label: "Headcount",
    color: "var(--brand)",
    points: occupancyPoints(),
  },
];

export const occupancyGapBands = [{ fromIndex: 7, toIndex: 13 }];

export const parcelActivity: ParcelActivity = {
  pointer: "-3,-2",
  series: occupancySeries,
  gapBands: occupancyGapBands,
  peak: s(4, `${E.sceneHistory}-3,-2&limit=5000`),
  occupied: s("38 of 1 214", `${E.sceneHistory}-3,-2&limit=5000`),
  historyBegins: s("2026-07-13 02:15Z", `${E.sceneHistory}-3,-2&limit=5000`),
  jumpUrl: "https://decentraland.org/jump/?position=-3,-2",
};

export const parcelNoHistory: ParcelActivity = {
  ...parcelActivity,
  pointer: "88,-91",
  noHistory: true,
};

export const indexDatums = {
  peopleInYourWorlds: l(5, E.liveData),
  networkPresence: s("22 peers, 8 islands", E.presenceCurrent),
  worlds: l(activityWorldRows as readonly ActivityWorldRow[], E.worlds),
  busiestScenes: s(busiestScenes as readonly BusiestRow[], E.presenceScenes),
  busiestWorlds: s(busiestWorlds as readonly BusiestRow[], E.presenceWorlds),
};

export const indexDatumsDegraded = {
  peopleInYourWorlds: unavailable(
    E.liveData,
    503,
    `GET ${WCS}/live-data returned 503.`,
  ),
  networkPresence: unavailable(
    E.presenceCurrent,
    null,
    `GET ${CATALYST}/presence/current did not respond.`,
  ),
  worlds: l(activityWorldRows as readonly ActivityWorldRow[], E.worlds),
  busiestScenes: unavailable(
    E.presenceScenes,
    null,
    `GET ${CATALYST}/presence/current/scenes did not respond.`,
  ),
  busiestWorlds: unavailable(
    E.presenceWorlds,
    null,
    `GET ${CATALYST}/presence/current/worlds did not respond.`,
  ),
};

export const emptyWorlds = l([] as readonly ActivityWorldRow[], E.worlds);

export const unavailableWorlds = unavailable(
  E.worlds,
  502,
  `GET ${WCS}/worlds?authorized_deployer= returned 502.`,
);

export const worldMeta: Datum<WorldMeta> = l(
  {
    title: "Pet Barn",
    owner: "0x313d\u{2026}9a1",
    lastDeployedAt: "2026-07-12T09:41:00Z",
    deployedScenes: 1,
    blockedSince: null,
  },
  E.worlds,
);

export const worldHistory: Datum<WorldHistory> = s(
  { series: occupancySeries, gapBands: occupancyGapBands },
  `${E.worldHistory}petbarn.dcl.eth&limit=5000`,
);

export const emptyWorldHistory: Datum<WorldHistory> = s(
  { series: [], gapBands: [] },
  `${E.worldHistory}petbarn.dcl.eth&limit=5000`,
);

export const unavailableWorldHistory = unavailable(
  `${E.worldHistory}petbarn.dcl.eth&limit=5000`,
  500,
  `GET ${CATALYST}/presence/worlds/history returned 500.`,
);

export const storageReading: Datum<StorageReading> = l(
  { label: "59.8 MB of 6.6 GB", ratio: 0.00906 },
  E.walletStats,
);

export const accessFacts: Datum<readonly FactRow[]> = l(
  [
    { label: "Owner", value: "0x313d\u{2026}9a1" },
    { label: "Deployment", value: "allow-list \u{B7} 2 wallets" },
    { label: "Streaming", value: "owner only" },
    { label: "Access", value: "unrestricted" },
  ] as readonly FactRow[],
  E.worldPermissions,
);

export const receptionFacts: Datum<readonly FactRow[]> = l(
  [
    { label: "Likes", value: "31" },
    { label: "Dislikes", value: "2" },
    { label: "Favourites", value: "14" },
    { label: "Like rate", value: "94%" },
    { label: "Listed on Places since", value: "12 Jul 2026" },
  ] as readonly FactRow[],
  E.places,
);

export const kvStorageUnavailable = unavailable(
  E.kvWorld,
  400,
  `GET ${CATALYST}/world-storage/usage/world returned 400 Invalid Auth Chain.`,
);

export const worldNotBuilt: NotBuiltSpec[] = [
  {
    id: "sessions",
    title: "Sessions & retention",
    why: "The client half of session and retention analytics exists in this repo with a generated zod model and a drift gate; the server route /creators/me/scenes/stats 404s. Presence persists addresses but its HTTP API returns counts only, so none of it is derivable here.",
    today: "the headcount and occupancy history above are the whole picture.",
  },
  {
    id: "errors",
    title: "Did it break?",
    why: "catalyrst-telemetry ingests Sentry-shaped events and groups them into issues, but every read is behind require_telemetry_admin and the data carries no scene or owner dimension. There is no query that means \u{201C}my world\u{201D}.",
    todayCli: {
      command:
        "# start your explorer with --mcp yourself first \u{2014} this does not launch it\ncurl -s http://127.0.0.1:8123/unity-explorer-mcp -d '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"get_scene_logs\",\"arguments\":{}}}'",
      explain:
        "dcl-scene-bots is an MCP client you run beside your own explorer. It catches TypeError \u{B7} ReferenceError \u{B7} Cannot read \u{B7} is not a function \u{B7} unhandled promise rejection.",
    },
  },
  {
    id: "notify",
    title: "Tell me when it changes",
    why: "catalyrst-notifications is email preferences (first_wear.rs, ports/email.rs) with zero parcel, land or scene references. There is nothing that could raise a scene event.",
    today: "the \u{27F3} button at the top of this page.",
  },
  {
    id: "scene-state",
    title: "Live 2-D scene state",
    why: "Nothing serves a scene's current entity or player state. Presence knows addresses and parcel coordinates internally; nothing exposes them, and /v2/parcels/{x}/{y} is a stub.",
    today: "join the world.",
  },
];

export const permissionsCli = {
  command:
    "dcl-one-sdk world permissions grant petbarn.dcl.eth deployment 0x313d\u{2026}9a1",
  explain:
    "PUT /world/{name}/permissions/deployment/{address} over an EIP-191 signed chain.",
};

const row = (
  id: string,
  datum: string,
  endpoint: string,
  usedBy: string[],
  note: string,
  probed?: Datum<unknown>,
): SourceLedgerRow => ({ id, datum, endpoint, usedBy, note, probed });

export const sourceGroups: SourceLedgerGroup[] = [
  {
    klass: "live",
    label: "Live",
    rows: [
      row(
        "wcs-worlds",
        "Your worlds",
        E.worlds,
        ["Activity", "Worlds"],
        "Real-time read. An empty array is a real answer and renders as an empty state, not as a failure.",
        l(activityWorldRows.length, E.worlds),
      ),
      row(
        "wcs-wallet-stats",
        "Deployed bytes + quota",
        E.walletStats,
        ["Activity \u{B7} world", "Worlds storage"],
        "Byte strings are decimal and parsed as BigInt, not Number.",
        l("59.8 MB of 6.6 GB", E.walletStats),
      ),
      row(
        "wcs-live-data",
        "Users online per world",
        E.liveData,
        ["Activity", "Activity \u{B7} world"],
        "The worlds server's own instant figure. Not the same measurement as presence.",
        l(5, E.liveData),
      ),
      row(
        "wcs-status",
        "Platform totals",
        E.status,
        ["Data sources"],
        "Network-wide, not scoped to you.",
        l("1 750 worlds", E.status),
      ),
      row(
        "places-worlds",
        "Likes / favourites",
        E.places,
        ["Activity \u{B7} world"],
        "user_visits and user_count from the same response are excluded \u{2014} see below.",
        l("31 likes", E.places),
      ),
      row(
        "world-about",
        "Scene URN + spawn",
        E.worldAbout,
        ["Activity \u{B7} world"],
        "Also the fallback for the world header when the wcs row is missing.",
        l("urn:decentraland:entity:\u{2026}", E.worldAbout),
      ),
      row(
        "world-permissions",
        "ACLs + owner",
        E.worldPermissions,
        ["Activity \u{B7} world"],
        "Rendered read-only. Writes belong to /creator-hub/world-permissions.",
        l("allow-list \u{B7} 2 wallets", E.worldPermissions),
      ),
      row(
        "realm-about",
        "Realm health",
        E.about,
        ["Activity \u{B7} world"],
        "healthy \u{B7} acceptingUsers \u{B7} synchronizationStatus.",
        l("healthy, accepting", E.about),
      ),
      row(
        "lambdas-names",
        "Your NAMEs",
        E.names,
        ["Worlds"],
        "The catalyst.example.com half of the split stack.",
        l(3, E.names),
      ),
    ],
  },
  {
    klass: "sampled",
    label: "Sampled",
    note: "A missing bucket is not a zero. Rows exist only for the instants a world or scene was live when the sampler ran. History depth is whatever exists, not a fixed window.",
    rows: [
      row(
        "presence-worlds",
        "Headcount per world",
        E.presenceWorlds,
        ["Activity", "Activity \u{B7} world"],
        "Cadence 300 s (PRESENCE_SNAPSHOT_INTERVAL_SECS).",
        s(2, E.presenceWorlds),
      ),
      row(
        "presence-scenes",
        "Headcount per scene",
        E.presenceScenes,
        ["Activity"],
        "Keyed by pointer. There is no owner field, which is why parcels cannot be listed.",
        s(4, E.presenceScenes),
      ),
      row(
        "presence-world-history",
        "Occupancy history per world",
        `${E.worldHistory}{name}&limit={n}`,
        ["Activity \u{B7} world"],
        "limit is clamped to [1, 5000] so the query string never lies about what was asked.",
        s(1214, `${E.worldHistory}{name}&limit={n}`),
      ),
      row(
        "presence-scene-history",
        "Occupancy history per scene",
        `${E.sceneHistory}{x,y}&limit={n}`,
        ["Activity \u{B7} parcel lookup"],
        "Same clamp.",
        s(5000, `${E.sceneHistory}{x,y}&limit={n}`),
      ),
    ],
  },
  {
    klass: "snapshot",
    label: "Snapshot",
    rows: [],
    emptyNote:
      "The vocabulary exists and nothing currently qualifies. A snapshot is only rendered when its export reports source: \"metabase\"; the artifact on disk reports source: \"fixture\", so it is filed under Unavailable instead.",
  },
  {
    klass: "unavailable",
    label: "Unavailable",
    rows: [
      row(
        "creators-stats",
        "Sessions \u{B7} retention \u{B7} device \u{B7} FPS (15 fields)",
        E.creatorsStats,
        ["nothing"],
        "404. That API serves /healthz, /api/auth/*, /api/me, /api/admin/* and /api/worlds/{w}/metrics, and nothing else.",
      ),
      row(
        "creators-metrics",
        "The 16 world metrics",
        E.creatorsMetrics,
        ["nothing"],
        "The host serves the marketing SPA, and the artifact on disk reports source: \"fixture\". Filed here, not under Snapshot: a synthetic export behind a warning chip is still a lie.",
      ),
      row(
        "world-storage-kv",
        "Scene key\u{2013}value storage (3 endpoints)",
        `GET ${CATALYST}/world-storage/usage/{world,players,env}`,
        ["Activity \u{B7} world (as an explanation)"],
        "400 Invalid Auth Chain. Needs an ADR-44 signed fetch from the scene runtime; a browser session cannot mint one. It is also a different number from deployed bytes.",
      ),
      row(
        "creators-me",
        "Creator identity \u{B7} studio \u{B7} tier",
        "GET creators-data/api/me",
        ["nothing"],
        "Not deployed.",
      ),
    ],
  },
  {
    klass: "unbuilt",
    label: "Not built",
    rows: [
      row(
        "federation",
        "World federation / submit to another realm",
        "\u{2014}",
        ["nothing"],
        "catalyrst-worlds has no sync or federation module, and federation-peers.toml holds one placeholder peer. Today: publishing updates this network only.",
      ),
      row(
        "land-notifications",
        "Land & parcel event notifications",
        "\u{2014}",
        ["nothing"],
        "catalyrst-notifications is email preferences with zero parcel, land or scene references. Today: the \u{27F3} button.",
      ),
      row(
        "custom-metrics",
        "Creator-defined metrics",
        "\u{2014}",
        ["nothing"],
        "The metric vocabulary is a 16-name whitelist enforced at import time; an unknown metric aborts the import.",
      ),
      row(
        "scoped-crashes",
        "Crash reports scoped to my scene",
        "\u{2014}",
        ["nothing"],
        "Telemetry reads are admin-gated and carry no scene or owner dimension. Today: read the explorer's own scene log over its --mcp port.",
      ),
      row(
        "bots",
        "In-scene bot spawn / control",
        "\u{2014}",
        ["nothing"],
        "No spawnBot/dropBot/requestBot anywhere. dcl-scene-bots is an MCP client you run beside your own explorer; the browser cannot reach that port.",
      ),
      row(
        "scene-state",
        "Live 2-D scene state",
        "\u{2014}",
        ["nothing"],
        "Nothing serves it. Today: join the world.",
      ),
    ],
  },
  {
    klass: "excluded",
    label: "Excluded on purpose",
    note: "These are never rendered anywhere in the hub. This list exists so nobody rediscovers them and wires them up.",
    rows: [
      row(
        "comms-users",
        "comms.usersCount / bff.userCount",
        `GET ${CATALYST}/about`,
        ["nothing"],
        "Both read 0 on both realms.",
      ),
      row(
        "hot-scenes",
        "Hot scenes",
        `GET ${CATALYST}/hot-scenes`,
        ["nothing"],
        "Fails open to [] on any upstream error, non-JSON or unreachable host \u{2014} so [] is unreadable.",
      ),
      row(
        "places-visits",
        "places user_visits / user_count",
        E.places,
        ["nothing"],
        "0 for every world sampled, including real, liked, occupied ones.",
      ),
      row(
        "v2-parcels",
        "Parcel detail",
        `GET ${CATALYST}/v2/parcels/{x}/{y}`,
        ["nothing"],
        "Stub: empty name and description, image URL http://127.0.0.1:5162.",
      ),
    ],
  },
];

export const sourceGroupsAllDown: SourceLedgerGroup[] = sourceGroups.map(
  (group) =>
    group.klass === "live" || group.klass === "sampled"
      ? {
          ...group,
          rows: group.rows.map((r) => ({
            ...r,
            probed: unavailable(
              r.endpoint,
              null,
              `${r.endpoint} did not respond.`,
            ),
          })),
        }
      : group,
);

export const unbuiltGenesis = unbuilt(
  "Your Genesis parcels",
  "Nothing on this stack maps a wallet to the parcels it deployed to.",
  "look a parcel up by coordinate.",
);
