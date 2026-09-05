import { Link, useRevalidator, useSearchParams } from "react-router";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import ChWorldActivityPage, {
  type FactRow,
  type NotBuiltSpec,
  type StorageReading,
  type WorldHistory,
  type WorldMeta,
} from "@ui/creatorhub/pages/ChWorldActivityPage";
import { showable, type Datum } from "@ui/creatorhub/lib/datum";

import UpstreamUnavailable from "@features/components/UpstreamUnavailable";
import { screenLedger } from "@features/components/creator-hub/source-ledger";
import { resolveBreadcrumbOrigin } from "@features/components/creator-hub/breadcrumbOrigins";

import {
  loadWorldActivity,
  type WorldActivityData,
} from "@data/lib/catalyst/creator-hub/activity.server";
import { PLACES_EXCLUDED_FIELDS_SENTENCE } from "@data/lib/catalyst/creator-hub/activity";
import {
  liveNow,
  noSample,
  sampleTime,
  sampledAt,
  unavailableBecause,
} from "@data/lib/catalyst/creator-hub/datum.server";
import { loadWalletStats } from "@data/lib/catalyst/wcs.server";
import {
  bytesFromString,
  findWorldSize,
  formatBytes,
  type WalletStats,
} from "@data/lib/catalyst/wcs";
import { SOURCE_REGISTRY } from "@data/lib/catalyst/creator-hub/data-sources";
import { worldJumpUrl } from "@data/lib/catalyst/places/presence";
import { readWallet } from "@data/lib/auth/wallet-cookie";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.activity_.$world";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = ({ params }: Route.MetaArgs) =>
  creatorHubMeta(`Activity \u{B7} ${params.world}`);

const STORY: StoryId = "creator-hub/world-activity";

const FALLBACK: Assignment = {
  variant: "default",
  flags: {},
  experimentKey: "ch_world_activity",
};

const SERIES_COLOR = "var(--brand)";

const KV_STORAGE_ENDPOINT =
  "GET catalyst.example.com/world-storage/usage/{world|players|env}";

const WCS_WORLDS_ENDPOINT =
  "GET worlds-content-server.decentraland.org/worlds?authorized_deployer=";

const WCS_WALLET_STATS_ENDPOINT =
  "GET worlds-content-server.decentraland.org/wallet/{address}/stats";

function worldMeta(d: WorldActivityData): Datum<WorldMeta> {
  const row = d.row;
  if (row) {
    // The wcs row is the richer answer and is the one the caller's own list
    // produced, so it wins when it exists.
    const value: WorldMeta = {
      title: row.title,
      owner: row.owner,
      lastDeployedAt: row.lastDeployedAt,
      deployedScenes: row.deployedScenes,
      blockedSince: row.blockedSince,
    };
    return showable(d.myWorlds)
      ? liveNow(value, d.myWorlds.endpoint, d.myWorlds.readAt)
      : liveNow(value, WCS_WORLDS_ENDPOINT);
  }

  // No wcs row: catalyst's /about knows the world exists but carries no owner,
  // title, deploy time or scene count. Those fields stay null rather than being
  // filled with a plausible default.
  if (!showable(d.about)) return d.about as Datum<WorldMeta>;
  const value: WorldMeta = {
    title: null,
    owner: null,
    lastDeployedAt: null,
    deployedScenes: null,
    blockedSince: null,
  };
  return liveNow(value, d.about.endpoint, d.about.readAt);
}

function realmLine(d: WorldActivityData): Datum<string> {
  if (!showable(d.realm)) return d.realm as Datum<string>;
  const r = d.realm.value;
  const parts: string[] = [];
  parts.push(r.healthy === null ? "health unknown" : r.healthy ? "healthy" : "unhealthy");
  parts.push(
    r.acceptingUsers === null
      ? "acceptance unknown"
      : r.acceptingUsers
        ? "accepting users"
        : "not accepting users",
  );
  if (r.content === null) parts.push("sync status unknown");
  else if (r.content.synchronizationStatus) parts.push(r.content.synchronizationStatus);
  return liveNow(parts.join(", "), d.realm.endpoint, d.realm.readAt);
}

function historyView(d: WorldActivityData): Datum<WorldHistory> {
  const h = d.history;
  if (!showable(h)) return h as Datum<WorldHistory>;
  const value: WorldHistory = {
    series: [
      {
        key: "occupancy",
        label: "People in this world",
        color: SERIES_COLOR,
        points: h.value.points,
      },
    ],
    gapBands: h.value.gapBands,
  };
  return sampledAt(
    value,
    h.endpoint,
    sampleTime(h),
    h.value.cadenceSeconds,
  );
}

const NO_HISTORY_NOTE =
  "The presence collector has no snapshots for this world \u{2014} it has not been in the poll set. That is not the same as a reading of zero.";

function peakDatum(d: WorldActivityData): Datum<number> {
  const h = d.history;
  if (!showable(h)) return h as Datum<number>;
  const takenAt = h.value.lastSeen ?? sampleTime(h);
  return h.value.peak === null
    ? noSample(h.endpoint, takenAt, NO_HISTORY_NOTE)
    : sampledAt(h.value.peak, h.endpoint, takenAt, h.value.cadenceSeconds);
}

function occupiedDatum(d: WorldActivityData): Datum<string> {
  const h = d.history;
  if (!showable(h)) return h as Datum<string>;
  const takenAt = h.value.lastSeen ?? sampleTime(h);
  if (h.value.sampleCount === 0) return noSample(h.endpoint, takenAt, NO_HISTORY_NOTE);
  return sampledAt(
    `${h.value.occupiedCount.toLocaleString("en-US")} of ${h.value.sampleCount.toLocaleString("en-US")}`,
    h.endpoint,
    takenAt,
    h.value.cadenceSeconds,
  );
}

function historyBeginsDatum(d: WorldActivityData): Datum<string> {
  const h = d.history;
  if (!showable(h)) return h as Datum<string>;
  const takenAt = h.value.lastSeen ?? sampleTime(h);
  return h.value.firstSeen === null
    ? noSample(h.endpoint, takenAt, NO_HISTORY_NOTE)
    : sampledAt(h.value.firstSeen, h.endpoint, takenAt, h.value.cadenceSeconds);
}

function sceneUrnDatum(d: WorldActivityData): Datum<string> {
  if (!showable(d.about)) return d.about as Datum<string>;
  const configurations = d.about.value.configurations;
  if (configurations === null) {
    return noSample(
      d.about.endpoint,
      d.about.readAt,
      "catalyst answered without a configurations block, so the scene URN was never in the response. That is not the same as nothing being deployed.",
    );
  }
  const urn = configurations.scenesUrn[0];
  if (!urn) {
    return noSample(
      d.about.endpoint,
      d.about.readAt,
      "catalyst answered, and listed no scene URN for this world. Nothing is deployed to it right now.",
    );
  }
  return liveNow(urn, d.about.endpoint, d.about.readAt);
}

function spawnDatum(d: WorldActivityData): Datum<string> {
  if (!showable(d.about)) return d.about as Datum<string>;
  const spawn = d.about.value.spawnCoordinates;
  if (!spawn) {
    return noSample(
      d.about.endpoint,
      d.about.readAt,
      "catalyst answered, and carried no spawn coordinates for this world.",
    );
  }
  return liveNow(spawn, d.about.endpoint, d.about.readAt);
}

/**
 * Deployed-content bytes for this NAME, against the wallet quota. Both figures
 * are decimal byte strings that can exceed `Number.MAX_SAFE_INTEGER`, so they
 * are parsed with BigInt and formatted, never `Number()`d.
 */
function storageDatum(
  world: string,
  stats: Datum<WalletStats>,
): Datum<StorageReading> {
  if (!showable(stats)) return stats as Datum<StorageReading>;
  const size = findWorldSize(stats.value, world);
  const max = bytesFromString(stats.value.maxAllowedSpace);
  if (size === null) {
    return noSample(
      stats.endpoint,
      stats.readAt,
      `worlds-content-server answered, and its per-NAME breakdown for this wallet does not list ${world}. That is not a reading of zero bytes.`,
    );
  }
  const label =
    max === null
      ? `${formatBytes(size)} deployed`
      : `${formatBytes(size)} of ${formatBytes(max)}`;
  const ratio = max === null || max === 0n ? null : Number(size) / Number(max);
  return liveNow({ label, ratio }, stats.endpoint, stats.readAt);
}

function accessRows(d: WorldActivityData): Datum<FactRow[]> {
  const p = d.permissions;
  if (!showable(p)) return p as Datum<FactRow[]>;
  const perms = p.value;
  // One row per permission, with the allow-list inline. An `allow-list` with no
  // wallets says "nobody but the owner" -- that is a real reading of the ACL, so
  // it is stated rather than hidden. A separate always-present allow-list row
  // would imply the list is meaningful even on an `unrestricted` world.
  const withList = (type: string, wallets: string[]) =>
    type === "allow-list"
      ? `${type} \u{2014} ${wallets.length === 0 ? "nobody but the owner" : wallets.join(", ")}`
      : type;
  const rows: FactRow[] = [
    { label: "Owner", value: perms.owner || "not reported" },
    {
      label: "Access",
      value: withList(
        perms.permissions.access.type,
        perms.permissions.access.wallets,
      ),
    },
    {
      label: "Deployment",
      value: withList(
        perms.permissions.deployment.type,
        perms.permissions.deployment.wallets,
      ),
    },
    {
      label: "Streaming",
      value: withList(
        perms.permissions.streaming.type,
        perms.permissions.streaming.wallets,
      ),
    },
  ];
  return liveNow(rows, p.endpoint, p.readAt);
}

const NO_VALUE_TEXT = "\u{2014}";

function receptionRows(d: WorldActivityData): Datum<FactRow[]> {
  const r = d.reception;
  if (!showable(r)) return r as Datum<FactRow[]>;
  const row = r.value[0];
  if (!row) {
    // A 200 with no rows: Places has no record of this world. Rendering four
    // zeros here would invent a reception the API never reported.
    return noSample(
      r.endpoint,
      r.readAt,
      "The Places API answered and has no record of this world, so there are no likes or favourites to show. Not the same as zero likes.",
    );
  }
  const n = (v: number | null) => (v === null ? NO_VALUE_TEXT : v.toLocaleString("en-US"));
  const rows: FactRow[] = [
    { label: "Likes", value: n(row.likes) },
    { label: "Dislikes", value: n(row.dislikes) },
    { label: "Favourites", value: n(row.favorites) },
    {
      label: "Like rate",
      value:
        row.like_rate === null
          ? NO_VALUE_TEXT
          : `${Math.round(row.like_rate * 100)}%`,
    },
    { label: "Listed since", value: row.deployed_at ?? NO_VALUE_TEXT },
    { label: "Not shown", value: PLACES_EXCLUDED_FIELDS_SENTENCE },
  ];
  return liveNow(rows, r.endpoint, r.readAt);
}

/**
 * The four "not built" panels. Every reason is lifted from the source registry
 * rather than retyped, so the ledger and the panels cannot drift apart.
 *
 * Built in the component, not the loader: `NotBuiltSpec.today` is a `ReactNode`
 * and does not survive loader serialization. It depends on nothing but the
 * world name and the registry, both of which the component already has.
 */
function notBuiltPanels(world: string): NotBuiltSpec[] {
  const entry = (id: string) => SOURCE_REGISTRY.find((e) => e.id === id);

  const sessions = entry("creators-scenes-stats");
  const crashes = entry("scene-crash-reports");
  const notify = entry("land-notifications");
  const sceneState = entry("live-scene-state");

  const specs: NotBuiltSpec[] = [];

  if (sessions) {
    specs.push({
      id: sessions.id,
      title: "Sessions & retention",
      why: sessions.note,
      today:
        "The headcounts above are the whole picture: presence returns counts, not identities.",
    });
  }
  if (crashes) {
    specs.push({
      id: crashes.id,
      title: "Did it break?",
      why: crashes.note,
      // No `todayCli` here on purpose. This panel previously offered
      // `python3 -m dclbots.run ...`, which is not a tool any creator has: it is
      // an unpublished harness that exists only on one developer's machine and
      // is not installable from anywhere. An escape hatch that cannot be run is
      // the same defect as a number that was never measured -- it just fails at
      // the creator's terminal instead of on the page.
      //
      // Restore a command here only when there is something a creator can
      // actually install. Until then the panel states the gap and stops.
    });
  }
  if (notify) {
    specs.push({
      id: notify.id,
      title: "Tell me when it changes",
      why: notify.note,
      today: "The \u{27F3} control at the top of this page.",
    });
  }
  if (sceneState) {
    specs.push({
      id: sceneState.id,
      title: "Live 2-D scene state",
      why: sceneState.note,
      today: "Join the world.",
    });
  }
  return specs;
}

export async function loader({ request, params }: Route.LoaderArgs) {
  const world = decodeURIComponent(params.world ?? "").trim();
  const url = new URL(request.url);
  // Scoping, not auth: occupancy is public and this page renders for any world.
  const address =
    url.searchParams.get("address")?.trim() || readWallet(request) || "";
  const from = url.searchParams.get("from");

  const { sid, assignment, wrap } = await storyLoader(request, STORY, FALLBACK);

  const [activity, stats] = await Promise.all([
    loadWorldActivity(world, { address, signal: request.signal }),
    address
      ? loadWalletStats(address, { signal: request.signal })
      : Promise.resolve(
          noSample(
            WCS_WALLET_STATS_ENDPOINT,
            new Date().toISOString(),
            "No address on this request, so the per-wallet storage breakdown was not read. Add ?address= to scope it.",
          ) as Datum<WalletStats>,
        ),
  ]);

  const notFound = !activity.worldKnown;
  const status = notFound ? 404 : activity.allUpstreamsDown ? 503 : 200;

  const payload = {
    sid,
    world,
    from,
    notFound,
    allUpstreamsDown: activity.allUpstreamsDown,
    deployedByCaller: !activity.foreign,
    jumpUrl: worldJumpUrl(world),
    readAt: activity.readAt,

    worldMeta: worldMeta(activity),
    inThisWorld: activity.now,
    commsRoom: activity.liveUsers,
    realm: realmLine(activity),

    history: historyView(activity),
    peak: peakDatum(activity),
    occupiedSnapshots: occupiedDatum(activity),
    historyBegins: historyBeginsDatum(activity),

    sceneUrn: sceneUrnDatum(activity),
    spawnCoordinates: spawnDatum(activity),
    storage: storageDatum(world, stats),
    // Permanently unavailable, and value-less by construction. It is gated
    // behind an ADR-44 signed fetch the browser cannot mint, AND it is a
    // different number from deployed bytes. Never a `0 B` tile.
    sceneKvStorage: unavailableBecause(
      KV_STORAGE_ENDPOINT,
      "It returns 400 Invalid Auth Chain. It needs an ADR-44 signed fetch made by the scene runtime (realm + parcel metadata); this hub holds no such identity. It would also be a different number \u{2014} that endpoint sums the key-value store your scene writes at runtime, not the bytes you deployed.",
    ) as Datum<unknown>,

    access: accessRows(activity),
    reception: receptionRows(activity),

    sources: screenLedger({
      usedBy: ["/creator-hub/activity/:world"],
      alsoIds: [
        "places-user-visits",
        "scene-crash-reports",
        "land-notifications",
        "live-scene-state",
        "creators-scenes-stats",
        "comms-users-count",
        "occupancy-totals",
      ],
      results: {
        "wcs-worlds": activity.myWorlds,
        "wcs-wallet-stats": stats,
        "wcs-live-data": activity.liveUsers,
        "presence-current-worlds": activity.now,
        "presence-world-history": activity.history,
        "world-about": activity.about,
        "world-permissions": activity.permissions,
        "realm-about": activity.realm,
        "places-worlds": activity.reception,
      },
    }),
  };

  return wrap(payload, { status });
}

export default function CreatorHubWorldActivityRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const revalidator = useRevalidator();
  const [searchParams] = useSearchParams();
  const origin = resolveBreadcrumbOrigin(d.from ?? "activity");

  if (d.allUpstreamsDown && !d.notFound) {
    return (
      <CreatorHubChrome active="activity">
        <UpstreamUnavailable
          title={`${d.world} is unreadable right now`}
          message="worlds-content-server, catalyst.example.com and the Places API all failed on this request. Nothing is shown rather than numbers we could not read."
          backHref={origin.to}
          backLabel={origin.label}
        />
      </CreatorHubChrome>
    );
  }

  const address = searchParams.get("address");
  const permissionsHref = `/creator-hub/world-permissions?world=${encodeURIComponent(d.world)}&from=activity${address ? `&address=${encodeURIComponent(address)}` : ""}`;

  return (
    <CreatorHubChrome active="activity">
      <ChWorldActivityPage
        world={d.world}
        worldMeta={d.worldMeta}
        jumpUrl={d.jumpUrl}
        readAt={d.readAt}
        inThisWorld={d.inThisWorld}
        commsRoom={d.commsRoom}
        realm={d.realm}
        history={d.history}
        peak={d.peak}
        occupiedSnapshots={d.occupiedSnapshots}
        historyBegins={d.historyBegins}
        onRetryHistory={() => revalidator.revalidate()}
        sceneUrn={d.sceneUrn}
        spawnCoordinates={d.spawnCoordinates}
        storage={d.storage}
        sceneKvStorage={d.sceneKvStorage}
        access={d.access}
        permissionsCli={{
          command: `dcl-one-sdk world permissions grant ${d.world} deployment 0x\u{2026}`,
          explain:
            "PUT /world/{name}/permissions/deployment/{address} over an EIP-191 signed auth chain. This page reads permissions and never writes them.",
        }}
        permissionsHref={permissionsHref}
        reception={d.reception}
        notBuilt={notBuiltPanels(d.world)}
        sources={d.sources}
        notFound={d.notFound}
        deployedByCaller={d.deployedByCaller}
        backTo={origin.to}
        backLabel={origin.label}
        LinkComponent={Link}
        worldsHref="/creator-hub/activity"
        onRefresh={() => revalidator.revalidate()}
        refreshing={revalidator.state === "loading"}
      />
    </CreatorHubChrome>
  );
}
