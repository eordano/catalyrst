import { useEffect, useRef } from "react";
import { useNavigate, useRevalidator } from "react-router";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import MetricsDashboardView from "@ui/creatorhub/pages/MetricsDashboardView";

import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";

import {
  fmtCount,
  type SceneVisitRow,
  type Summary,
} from "@data/lib/catalyst/creator-hub/metrics";
import { loadCreatorMetrics } from "@data/lib/catalyst/creator-hub/metrics.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { track } from "@core/lib/telemetry/track";
import {
  CREATOR_DASHBOARD_VIEWED,
  CREATOR_FUNNEL_STORY,
} from "@core/lib/telemetry/creator-funnel";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.metrics";

export const meta = () => creatorHubMeta("Metrics");

const STORY = CREATOR_FUNNEL_STORY;

const FALLBACK: Assignment = {
  variant: "dashboard",
  flags: { dashboard: true },
  experimentKey: "creator_metrics_dashboard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const address =
    url.searchParams.get("address")?.trim() || readWallet(request) || "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const metrics = address
    ? await loadCreatorMetrics(address, request.signal).catch(() => null)
    : null;

  const payload = {
    sid,
    address,
    noAddress: !address,
    windowDays: metrics?.windowDays ?? 7,
    summary: metrics?.summary ?? {
      publishedCollections: null,
      onSaleItems: null,
      sales7d: null,
      salesVolumeMana7d: null,
      salesUnavailable: true,
      scenes: null,
    },
    sceneRows: metrics?.sceneRows ?? [],
    empty: !!metrics?.empty,
    loadError: !!address && (!metrics || metrics.loadError),
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  address: string;
  noAddress: boolean;
  windowDays: number;
  summary: Summary;
  sceneRows: SceneVisitRow[];
  empty: boolean;
  loadError: boolean;
};

export default function CreatorHubMetricsRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);

  return (
    <CreatorHubChrome
      active="metrics"
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={() => {
        openSignIn();
      }}
    >
      <MetricsDashboard
        sid={d.sid}
        address={d.address}
        noAddress={d.noAddress}
        windowDays={d.windowDays}
        summary={d.summary}
        sceneRows={d.sceneRows}
        empty={d.empty}
        loadError={d.loadError}
        connected={isConnected}
        connectedAddress={address ?? ""}
        onConnect={() => openSignIn()}
      />
    </CreatorHubChrome>
  );
}

type DashboardProps = LoaderData & {
  connected: boolean;
  connectedAddress: string;
  onConnect: () => void;
};

function MetricsDashboard({
  sid,
  address,
  noAddress,
  windowDays,
  summary,
  sceneRows,
  empty,
  loadError,
  connected,
  connectedAddress,
  onConnect,
}: DashboardProps) {
  const navigate = useNavigate();
  const revalidator = useRevalidator();

  useEffect(() => {
    if (connected && connectedAddress && noAddress) {
      void navigate(`?address=${encodeURIComponent(connectedAddress)}`, {
        replace: true,
        preventScrollReset: true,
      });
    }
  }, [connected, connectedAddress, noAddress, navigate]);

  const rescoping = connected && Boolean(connectedAddress) && noAddress;

  useDashboardViewed(sid, windowDays, summary);

  const retrying = revalidator.state === "loading";
  function onRetry() {
    if (typeof revalidator.revalidate === "function") {
      revalidator.revalidate();
    } else if (address) {
      void navigate(`?address=${encodeURIComponent(address)}`, {
        replace: true,
        preventScrollReset: true,
      });
    } else {
      void navigate(0);
    }
  }

  const cards: SummaryCardData[] | null =
    loadError || empty
      ? null
      : [
          summary.publishedCollections === null
            ? {
                label: "Published collections",
                value: "Not available",
                note: "Couldn\u{2019}t load your collections right now.",
                unavailable: true,
              }
            : {
                label: "Published collections",
                value: fmtCount(summary.publishedCollections),
              },
          summary.onSaleItems === null
            ? {
                label: "Items on sale",
                value: "Not available",
                note: "Couldn\u{2019}t load your catalog items right now.",
                unavailable: true,
              }
            : {
                label: "Items on sale",
                value: fmtCount(summary.onSaleItems),
              },
          summary.sales7d === null || summary.salesUnavailable
            ? {
                label: `Sales (${windowDays}d)`,
                value: "Not available",
                note: "Couldn\u{2019}t load your sales right now.",
                unavailable: true,
              }
            : {
                label: `Sales (${windowDays}d)`,
                value: fmtCount(summary.sales7d),
                note:
                  summary.salesVolumeMana7d && summary.salesVolumeMana7d > 0
                    ? `${fmtCount(summary.salesVolumeMana7d)} MANA volume`
                    : undefined,
              },
          summary.scenes === null
            ? {
                label: "Scene visits (30d)",
                value: "Not available",
                note: "Couldn\u{2019}t load your places right now.",
                unavailable: true,
              }
            : summary.scenes.places === 0
              ? {
                  label: "Scene visits (30d)",
                  value: "No scenes yet",
                  note: "Publish a scene and its visits will count here.",
                  unavailable: true,
                }
              : {
                  label: "Scene visits (30d)",
                  value: fmtCount(summary.scenes.visits30d),
                  note: `${fmtCount(summary.scenes.liveNow)} in your scenes now \u{B7} ${fmtCount(summary.scenes.places)} ${summary.scenes.places === 1 ? "place" : "places"} listed`,
                },
        ];

  return (
    <MetricsDashboardView
      windowDays={windowDays}
      noAddress={noAddress}
      rescoping={rescoping}
      loadError={loadError}
      retrying={retrying}
      empty={empty}
      cards={cards}
      sceneRows={sceneRows.map((r) => ({
        ...r,
        location: r.location ?? "location unknown",
      }))}
      onConnect={onConnect}
      onRetry={onRetry}
    />
  );
}

type SummaryCardData = {
  label: string;
  value: string;
  note?: string;
  unavailable?: boolean;
};

function useDashboardViewed(
  sid: string,
  windowDays: number,
  summary: Summary,
) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(
      CREATOR_DASHBOARD_VIEWED,
      {
        window_days: windowDays,
        published_collections: summary.publishedCollections,
        on_sale_items: summary.onSaleItems,
        sales_7d: summary.sales7d,
        scene_visits_30d: summary.scenes?.visits30d ?? null,
      },
      { sid, story: STORY },
    );
  }, [sid, windowDays, summary]);
}
