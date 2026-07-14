import { useNavigate, useRevalidator, useSearchParams } from "react-router";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import ChActivityIndexPage, {
  type ActivityWorldRow as ActivityWorldRowView,
  type BusiestRow,
  type ParcelActivity,
} from "@ui/creatorhub/pages/ChActivityIndexPage";
import {
  showable,
  type Datum,
} from "@ui/creatorhub/lib/datum";

import UpstreamUnavailable from "@features/components/UpstreamUnavailable";
import { screenLedger } from "@features/components/creator-hub/source-ledger";
import { openSignIn } from "@features/components/auth/signin-store";

import {
  loadActivityIndex,
  loadSceneActivity,
  type ActivityIndexData,
  type SceneActivityData,
} from "@data/lib/catalyst/creator-hub/activity.server";
import {
  liveNow,
  noSample,
  sampleTime,
  sampledAt,
} from "@data/lib/catalyst/creator-hub/datum.server";
import { sceneJumpUrl, worldJumpUrl } from "@data/lib/catalyst/places/presence";
import { useAuth } from "@data/lib/auth/index";
import { readWallet } from "@data/lib/auth/wallet-cookie";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.activity";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Activity");

const STORY: StoryId = "creator-hub/activity";

const FALLBACK: Assignment = {
  variant: "default",
  flags: {},
  experimentKey: "ch_activity",
};

const BUSIEST_LIMIT = 8;

/* ------------------------------------------------------------ derivations --
 * These run in the loader, on the server, because they build `Datum`s and the
 * only sanctioned constructors live behind `datum.server.ts`. Each one either
 * derives from a reading it actually has, or hands back the very datum that
 * blocked it -- so the reason the screen shows `--` always names a real endpoint.
 */

/**
 * The sum of `/live-data`'s per-world figures across the caller's worlds.
 *
 * Deliberately NOT `data.totalUsers`: that is every world on the platform, and
 * labelling it "people in your worlds" would be the exact kind of quiet
 * relabelling this feature exists to stop. Needs both the world list and
 * `/live-data`; without either it propagates the datum that is missing rather
 * than summing over a partial list.
 */
function peopleInYourWorlds(d: ActivityIndexData): Datum<number> {
  if (!showable(d.liveData)) return d.liveData as Datum<number>;
  if (!showable(d.worlds)) return d.worlds as Datum<number>;
  if (d.rows.length === 0) {
    // A sum over no worlds is not a measurement of nobody. Reporting `0` here
    // would read as "the worlds server looked and found no one", when in fact
    // there was nothing to look at.
    return noSample(
      d.liveData.endpoint,
      d.liveData.readAt,
      "No worlds are deployed under this address, so there is nothing to count. Not a reading of zero people.",
    );
  }
  let total = 0;
  for (const row of d.rows) {
    if (!showable(row.liveUsers)) return row.liveUsers as Datum<number>;
    total += row.liveUsers.value;
  }
  return liveNow(total, d.liveData.endpoint, d.liveData.readAt);
}

/** `"22 peers, 8 islands"`, rendered verbatim. */
function networkPresence(d: ActivityIndexData): Datum<string> {
  const current = d.current;
  if (!showable(current)) return current as Datum<string>;
  const { peers_count: peers, islands_count: islands } = current.value;
  const text = `${peers.toLocaleString("en-US")} peers, ${islands.toLocaleString("en-US")} islands`;
  return current.state === "sampled"
    ? sampledAt(text, current.endpoint, current.takenAt, current.cadenceSeconds)
    : liveNow(text, current.endpoint, current.readAt);
}

function worldRows(d: ActivityIndexData): Datum<ActivityWorldRowView[]> {
  const worlds = d.worlds;
  if (!showable(worlds)) return worlds as Datum<ActivityWorldRowView[]>;
  const rows: ActivityWorldRowView[] = d.rows.map((row) => ({
    name: row.world.name,
    title: row.world.title,
    lastDeployedAt: row.world.lastDeployedAt,
    deployedScenes: row.world.deployedScenes,
    blockedSince: row.world.blockedSince,
    now: row.now,
    nowNote: row.nowNote,
    peak7d: row.peak7d,
    href: `/creator-hub/activity/${encodeURIComponent(row.world.name)}`,
    jumpUrl: row.jumpUrl,
    publishHref: `/creator-hub/deploy-world?world=${encodeURIComponent(row.world.name)}&from=activity`,
  }));
  return liveNow(rows, worlds.endpoint, worlds.readAt);
}

/**
 * `busiest` lists drop rows whose count is 0. That is a presentation choice on
 * a "busiest right now" list, not a repair: an all-zero snapshot yields an
 * empty showable list, which the page renders as "the last snapshot found
 * nobody anywhere in this realm -- that is a reading, not a failure".
 */
function busiestScenes(d: ActivityIndexData): Datum<BusiestRow[]> {
  const scenes = d.presenceScenes;
  if (!showable(scenes)) return scenes as Datum<BusiestRow[]>;
  const rows: BusiestRow[] = [...scenes.value]
    .filter((r) => r.count > 0)
    .sort((a, b) => b.count - a.count)
    .slice(0, BUSIEST_LIMIT)
    .map((r) => ({
      key: r.pointer,
      label: r.scene_name || r.pointer,
      sub: r.scene_name ? r.pointer : null,
      count: r.count,
      href: sceneJumpUrl(r.pointer),
    }));
  return sampledDerived(scenes, rows);
}

function busiestWorlds(d: ActivityIndexData): Datum<BusiestRow[]> {
  const worlds = d.presenceWorlds;
  if (!showable(worlds)) return worlds as Datum<BusiestRow[]>;
  const rows: BusiestRow[] = [...worlds.value]
    .filter((r) => r.count > 0)
    .sort((a, b) => b.count - a.count)
    .slice(0, BUSIEST_LIMIT)
    .map((r) => ({
      key: r.world_name,
      label: r.world_name,
      sub: null,
      count: r.count,
      href: worldJumpUrl(r.world_name),
    }));
  return sampledDerived(worlds, rows);
}

/** Re-wraps a derived value in the provenance of the reading it came from. */
function sampledDerived<S, T>(source: Datum<S>, value: T): Datum<T> {
  if (source.state === "sampled") {
    return sampledAt(
      value,
      source.endpoint,
      source.takenAt,
      source.cadenceSeconds,
    );
  }
  if (source.state === "live") {
    return liveNow(value, source.endpoint, source.readAt);
  }
  return source as Datum<T>;
}

const SERIES_COLOR = "var(--brand)";

/** The `?pointer=x,y` lookup: a parcel can be looked up, never listed. */
function parcelView(scene: SceneActivityData): ParcelActivity {
  const history = scene.history;
  const jumpUrl = sceneJumpUrl(scene.pointer);

  if (!showable(history)) {
    return {
      pointer: scene.pointer,
      series: [],
      gapBands: [],
      peak: history as Datum<number>,
      occupied: history as Datum<string>,
      historyBegins: history as Datum<string>,
      noHistory: true,
      jumpUrl,
    };
  }

  const h = history.value;
  const takenAt = h.lastSeen ?? sampleTime(history);
  const cadence = h.cadenceSeconds;
  const noneNote =
    "The presence collector has no snapshots for this pointer, so there is nothing to derive. Not the same as a reading of zero.";

  return {
    pointer: scene.pointer,
    series: [
      {
        key: "occupancy",
        label: "People here",
        color: SERIES_COLOR,
        points: h.points,
      },
    ],
    gapBands: h.gapBands,
    // `peak === null` means no sample at all. A peak of 0 would read as
    // "nobody ever came", which is a different fact, so it is never coerced.
    peak:
      h.peak === null
        ? noSample(history.endpoint, takenAt, noneNote)
        : sampledAt(h.peak, history.endpoint, takenAt, cadence),
    occupied: sampledAt(
      `${h.occupiedCount.toLocaleString("en-US")} of ${h.sampleCount.toLocaleString("en-US")}`,
      history.endpoint,
      takenAt,
      cadence,
    ),
    historyBegins:
      h.firstSeen === null
        ? noSample(history.endpoint, takenAt, noneNote)
        : sampledAt(h.firstSeen, history.endpoint, takenAt, cadence),
    noHistory: h.sampleCount === 0,
    jumpUrl,
  };
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  // Scoping, not auth. No route in this app rejects an unauthenticated request
  // and `dcl_wallet` is written client-side; the address only picks rows.
  const address =
    url.searchParams.get("address")?.trim() || readWallet(request) || "";
  const pointer = url.searchParams.get("pointer")?.trim() || "";

  const { sid, assignment, wrap } = await storyLoader(request, STORY, FALLBACK);

  const index = await loadActivityIndex({ address, signal: request.signal });
  const scene = pointer
    ? await loadSceneActivity(pointer, { signal: request.signal })
    : null;

  const payload = {
    sid,
    address: address || null,
    readAt: index.readAt,
    peopleInYourWorlds: peopleInYourWorlds(index),
    networkPresence: networkPresence(index),
    worlds: worldRows(index),
    busiestScenes: busiestScenes(index),
    busiestWorlds: busiestWorlds(index),
    parcelPointer: pointer || null,
    parcel: scene ? parcelView(scene) : null,
    sources: screenLedger({
      usedBy: pointer
        ? ["/creator-hub/activity", "/creator-hub/activity?pointer="]
        : ["/creator-hub/activity"],
      // Endpoints this screen touched and deliberately does not render, plus
      // the ones nobody should rediscover and wire up.
      alsoIds: [
        "creators-scenes-stats",
        "hot-scenes",
        "comms-users-count",
        "occupancy-totals",
        "worlds-base",
        "v2-parcels",
      ],
      results: {
        "wcs-worlds": index.worlds,
        "wcs-live-data": index.liveData,
        "presence-current-worlds": index.presenceWorlds,
        "presence-current-scenes": index.presenceScenes,
        "presence-world-history": index.rows[0]?.peak7d,
        ...(scene ? { "presence-scene-history": scene.history } : {}),
      },
    }),
    allUpstreamsDown: index.allUpstreamsDown,
  };

  return wrap(payload, { status: index.allUpstreamsDown ? 503 : 200 });
}

export default function CreatorHubActivityRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const navigate = useNavigate();
  const revalidator = useRevalidator();
  const [searchParams] = useSearchParams();
  const { isConnected, address } = useAuth();

  if (d.allUpstreamsDown) {
    return (
      <CreatorHubChrome active="activity">
        <UpstreamUnavailable
          title="Activity is unreadable right now"
          message="worlds-content-server and catalyst.example.com/presence both failed on this request. Rather than show you numbers we could not read, this page is showing nothing. Try again in a moment."
          backHref="/creator-hub/manage"
          backLabel="Back to Worlds"
        />
      </CreatorHubChrome>
    );
  }

  function setParam(key: string, value: string | null) {
    const sp = new URLSearchParams(searchParams);
    if (value) sp.set(key, value);
    else sp.delete(key);
    void navigate({ search: `?${sp.toString()}` }, { preventScrollReset: true });
  }

  return (
    <CreatorHubChrome active="activity">
      <ChActivityIndexPage
        address={d.address}
        readAt={d.readAt}
        peopleInYourWorlds={d.peopleInYourWorlds}
        networkPresence={d.networkPresence}
        worlds={d.worlds}
        busiestScenes={d.busiestScenes}
        busiestWorlds={d.busiestWorlds}
        parcel={d.parcel}
        parcelPointer={d.parcelPointer}
        sources={d.sources}
        onRefresh={() => revalidator.revalidate()}
        refreshing={revalidator.state === "loading"}
        onConnect={() => {
          if (isConnected && address) setParam("address", address);
          else openSignIn();
        }}
        onAddressSubmit={(next) => setParam("address", next)}
        onPointerLookup={(next) => setParam("pointer", next)}
      />
    </CreatorHubChrome>
  );
}
