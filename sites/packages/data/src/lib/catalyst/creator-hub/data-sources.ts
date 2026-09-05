/*
 * The source ledger: every datum the creator hub can show, its state, and the
 * endpoint behind it -- including the ones deliberately NOT displayed and why.
 *
 * The registry rows here are constants with no probes attached, so this module
 * stays client-safe. `data-sources.server.ts` attaches a probe to every `live`
 * and `sampled` row and runs them, which is what stops the ledger from claiming
 * "live" for something that is down.
 */

export type SourceClass =
  | "live"
  | "sampled"
  | "snapshot"
  | "unavailable"
  | "unbuilt"
  | "excluded";

export type SourceEntry = {
  id: string;
  /** what the datum is, in the creator's words */
  datum: string;
  /** `GET host/path`, verbatim */
  endpoint: string;
  /** screens that read it */
  usedBy: string[];
  klass: SourceClass;
  note: string;
  /** only for `unbuilt` rows: the escape hatch that exists today */
  today?: string;
};

export const SOURCE_GROUP_ORDER: SourceClass[] = [
  "live",
  "sampled",
  "snapshot",
  "unavailable",
  "unbuilt",
  "excluded",
];

export const SOURCE_GROUP_NOTES: Partial<Record<SourceClass, string>> = {
  snapshot:
    "The vocabulary exists and nothing currently qualifies. A snapshot is a dated export from metabase; an artifact that reports any other source is not rendered at all.",
  sampled:
    "A missing bucket is not a zero. Rows exist only for the instants a world or scene was live when the sampler ran. History depth is whatever exists, not a fixed window.",
  excluded:
    "These are never rendered anywhere in the hub. This list exists so nobody rediscovers them and wires them up.",
};

const WCS = "worlds-content-server.decentraland.org";
const CAT = "catalyst.example.com";

export const SOURCE_REGISTRY: SourceEntry[] = [
  {
    id: "wcs-worlds",
    datum: "Your worlds",
    endpoint: `GET ${WCS}/worlds?authorized_deployer={address}`,
    usedBy: ["/creator-hub/activity", "/creator-hub/manage"],
    klass: "live",
    note: "Worlds worlds-content-server holds content for, filtered to the address you supplied. A 200 with no rows is a real answer and is shown as an empty list, never as a failure.",
  },
  {
    id: "wcs-wallet-stats",
    datum: "Deployed bytes and quota",
    endpoint: `GET ${WCS}/wallet/{address}/stats`,
    usedBy: ["/creator-hub/activity/:world", "/creator-hub/worlds-storage"],
    klass: "live",
    note: "Deployed-content bytes per NAME plus the wallet allowance. Byte counts are decimal strings and are parsed with BigInt. Worlds deployed to catalyst.example.com are not counted here.",
  },
  {
    id: "wcs-live-data",
    datum: "Users online per world",
    endpoint: `GET ${WCS}/live-data`,
    usedBy: ["/creator-hub/activity", "/creator-hub/activity/:world"],
    klass: "live",
    note: "The worlds server's own instant figure. It lists only rooms that currently have users, and it routinely disagrees with the presence sample \u{2014} both are shown, neither is reconciled.",
  },
  {
    id: "wcs-status",
    datum: "Platform totals",
    endpoint: `GET ${WCS}/status`,
    usedBy: ["/creator-hub/data-sources"],
    klass: "live",
    note: "World counts and comms room/user totals for the upstream network.",
  },
  {
    id: "places-worlds",
    datum: "Likes, dislikes, favourites",
    endpoint: `GET ${CAT}/places/api/worlds?names={world}`,
    usedBy: ["/creator-hub/activity/:world"],
    klass: "live",
    note: "Reception for one world. A 200 with an empty list means Places has no record of it \u{2014} rendered as an empty state, not as zeros.",
  },
  {
    id: "world-about",
    datum: "Scene URN and spawn point",
    endpoint: `GET ${CAT}/world/{world}/about`,
    usedBy: ["/creator-hub/activity/:world"],
    klass: "live",
    note: "404 here means either 'no scenes deployed' or 'never heard of it' \u{2014} catalyst answers the same way for both, so the page does not claim to know which.",
  },
  {
    id: "world-permissions",
    datum: "Access control list and owner",
    endpoint: `GET ${CAT}/world/{world}/permissions`,
    usedBy: ["/creator-hub/activity/:world"],
    klass: "live",
    note: "Read-only here. Changing it is a signed write that belongs to /creator-hub/world-permissions.",
  },
  {
    id: "realm-about",
    datum: "Realm health",
    endpoint: `GET ${CAT}/about`,
    usedBy: ["/creator-hub/activity", "/creator-hub/activity/:world"],
    klass: "live",
    note: "Whether the realm is healthy, accepting users, and in sync.",
  },
  {
    id: "lambdas-names",
    datum: "Your NAMEs",
    endpoint: `GET ${CAT}/lambdas/users/{address}/names`,
    usedBy: ["/creator-hub/manage"],
    klass: "live",
    note: "NAMEs owned on this stack. A NAME can exist with nothing deployed to it, so this list and the worlds-content-server list disagree by design.",
  },

  {
    id: "presence-current-worlds",
    datum: "Headcount per world",
    endpoint: `GET ${CAT}/presence/current/worlds`,
    usedBy: ["/creator-hub/activity", "/creator-hub/activity/:world"],
    klass: "sampled",
    note: "The most recent 5-minute snapshot. A world missing from it was not live when the sampler ran \u{2014} that is 'no sample', not zero.",
  },
  {
    id: "presence-current-scenes",
    datum: "Headcount per scene",
    endpoint: `GET ${CAT}/presence/current/scenes`,
    usedBy: ["/creator-hub/activity"],
    klass: "sampled",
    note: "Occupancy by parcel pointer. There is no owner field on these rows, which is why parcels can be looked up but never listed.",
  },
  {
    id: "presence-world-history",
    datum: "Occupancy history per world",
    endpoint: `GET ${CAT}/presence/worlds/history?world={world}&limit={limit}`,
    usedBy: ["/creator-hub/activity", "/creator-hub/activity/:world"],
    klass: "sampled",
    note: "Whatever snapshots exist, newest first, capped at 5000 rows. Empty cadence buckets are plotted as gaps, never as zeros, and the series never extends past the last sample.",
  },
  {
    id: "presence-scene-history",
    datum: "Occupancy history per scene",
    endpoint: `GET ${CAT}/presence/scenes/history?pointer={x,y}&limit={limit}`,
    usedBy: ["/creator-hub/activity?pointer="],
    klass: "sampled",
    note: "The Genesis-parcel lookup. Same gap rules as the world history.",
  },

  {
    id: "creators-scenes-stats",
    datum: "Sessions, retention, dwell time, device split, FPS (15 fields)",
    endpoint: "GET decentraland.org/creators-data/api/creators/me/scenes/stats",
    usedBy: ["/creator-hub/scene-analytics"],
    klass: "unavailable",
    note: "404. That API serves /healthz, /api/auth/*, /api/me, /api/admin/* and /api/worlds/{w}/metrics \u{2014} this route is not among them. The client half exists in this repo with a generated zod model and a drift gate, and stays unused until the route does.",
  },
  {
    id: "creators-world-metrics",
    datum: "The 16 world metrics",
    endpoint: "GET decentraland.org/creators-data/api/worlds/{world}/metrics",
    usedBy: [],
    klass: "unavailable",
    note: "The host serves the marketing SPA, so a JSON read gets HTML. The artifact behind it reports source: fixture (synthetic) \u{2014} which would still not be rendered, because only a metabase export may become a snapshot. No retry control: a retry cannot succeed.",
  },
  {
    id: "world-storage-usage",
    datum: "Scene key-value storage (world, players, env)",
    endpoint: `GET ${CAT}/world-storage/usage/{world|players|env}`,
    usedBy: ["/creator-hub/worlds-storage", "/creator-hub/activity/:world"],
    klass: "unavailable",
    note: "400 Invalid Auth Chain. It needs an ADR-44 signed fetch made by the scene runtime (realm + parcel metadata); this hub holds no such identity. It would also be a different number \u{2014} that endpoint sums the KV store your scene writes at runtime, not the bytes you deployed.",
  },
  {
    id: "creators-me",
    datum: "Creator identity, studio, tier",
    endpoint: "GET decentraland.org/creators-data/api/me",
    usedBy: [],
    klass: "unavailable",
    note: "Not deployed. The host answers with the marketing SPA.",
  },

  {
    id: "world-federation",
    datum: "World federation / submit to another realm",
    endpoint: "\u{2014}",
    usedBy: [],
    klass: "unbuilt",
    note: "catalyrst-worlds has no sync or federation module, the Genesis replication stack has no world references, and the federation peer list holds one placeholder peer. interconnected.online is a real content server whose /worlds returns 0 rows and which has no per-scene comms room, so there is nothing to submit to.",
    today:
      "Publishing from this hub updates this network only, not Genesis City on decentraland.org.",
  },
  {
    id: "land-notifications",
    datum: "Land and parcel event notifications",
    endpoint: "\u{2014}",
    usedBy: [],
    klass: "unbuilt",
    note: "catalyrst-notifications is email preferences \u{2014} its substance is first-wear and email ports, with zero parcel, land or scene references.",
    today: "The refresh control on the activity pages.",
  },
  {
    id: "creator-metrics",
    datum: "Creator-defined metrics",
    endpoint: "\u{2014}",
    usedBy: [],
    klass: "unbuilt",
    note: "The metric vocabulary is a 16-name whitelist enforced at import time; an unknown metric aborts the import, and the endpoint reads a pre-exported snapshot. A creator can read what someone else exported and cannot define one.",
  },
  {
    id: "scene-crash-reports",
    datum: "Crash reports scoped to my scene",
    endpoint: "\u{2014}",
    usedBy: [],
    klass: "unbuilt",
    note: "catalyrst-telemetry ingests Sentry-shaped events and groups them into issues, but every read is behind an admin guard and the data carries no scene or owner dimension. There is no query that means 'my world'.",
    // get_scene_logs is on the explorer's own MCP surface, so it works for
    // anyone already running with --mcp -- no separate unpublished tooling.
    today:
      "Start your explorer with --mcp, then read its scene log directly: POST get_scene_logs to http://127.0.0.1:8123/unity-explorer-mcp.",
  },
  {
    id: "in-scene-bots",
    datum: "In-scene bot spawn and control",
    endpoint: "\u{2014}",
    usedBy: [],
    klass: "unbuilt",
    note: "No spawn/drop/request bot path exists anywhere in the server, sites or the play harness. dcl-scene-bots is an MCP client you run beside your own explorer; the browser cannot reach that port.",
    today: "Run it yourself from a terminal \u{2014} the command is on the world page.",
  },
  {
    id: "live-scene-state",
    datum: "Live 2-D scene state / who is where",
    endpoint: "\u{2014}",
    usedBy: [],
    klass: "unbuilt",
    note: "Nothing serves a scene's current entity or player state. Presence knows addresses and parcel coordinates internally; nothing exposes them.",
    today: "Join the world.",
  },
  {
    id: "parcel-ownership",
    datum: "Which Genesis parcels are mine",
    endpoint: "\u{2014}",
    usedBy: ["/creator-hub/activity"],
    klass: "unbuilt",
    note: "Nothing on this stack maps a wallet to the parcels it deployed to. /presence/current/scenes reports occupancy by pointer with no owner field, and worlds-content-server's authorized_deployer filter covers worlds only.",
    today: "Look up a pointer directly with ?pointer=x,y.",
  },

  {
    id: "comms-users-count",
    datum: "comms.usersCount / bff.userCount",
    endpoint: `GET ${CAT}/about`,
    usedBy: [],
    klass: "excluded",
    note: "Both read 0 on both realms, on a network that demonstrably has users.",
  },
  {
    id: "hot-scenes",
    datum: "Hot scenes",
    endpoint: `GET ${CAT}/hot-scenes`,
    usedBy: [],
    klass: "excluded",
    note: "Returns [] and fails open to [] on any upstream error, non-JSON body or unreachable host \u{2014} so an empty answer is indistinguishable from a broken one.",
  },
  {
    id: "places-user-visits",
    datum: "places user_visits / user_count",
    endpoint: `GET ${CAT}/places/api/worlds?names={world}`,
    usedBy: [],
    klass: "excluded",
    note: "0 for every world sampled, including real, liked, occupied ones.",
  },
  {
    id: "v2-parcels",
    datum: "Parcel detail",
    endpoint: `GET ${CAT}/v2/parcels/{x}/{y}`,
    usedBy: [],
    klass: "excluded",
    note: "A stub: empty name and description, and an image URL pointing at 127.0.0.1.",
  },
  {
    id: "occupancy-totals",
    datum: "occupancyTotals() for a per-world figure",
    endpoint: "\u{2014}",
    usedBy: [],
    klass: "excluded",
    note: "It Math.maxes three disagreeing sources into one number. The hub shows the sources side by side instead.",
  },
  {
    id: "worlds-base",
    datum: "worldsBase() as a worlds host",
    endpoint: "GET worlds.example.com/*",
    usedBy: [],
    klass: "excluded",
    note: "It rewrites the catalyst hostname to worlds.<domain>, which 404s every path. worlds-content-server.decentraland.org is a different, real host.",
  },
];

export function sourcesByClass(
  entries: SourceEntry[] = SOURCE_REGISTRY,
): { klass: SourceClass; note: string | null; entries: SourceEntry[] }[] {
  return SOURCE_GROUP_ORDER.map((klass) => ({
    klass,
    note: SOURCE_GROUP_NOTES[klass] ?? null,
    entries: entries.filter((e) => e.klass === klass),
  }));
}

export function isProbeable(klass: SourceClass): boolean {
  return klass === "live" || klass === "sampled";
}
