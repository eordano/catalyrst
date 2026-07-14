import path from "node:path";

import { useEffect, useRef } from "react";
import { Link, useSearchParams } from "react-router";

import OpDashboardPage from "@ui/operator/pages/OpDashboardPage";

import { coerceRange, totals, type Range } from "@data/lib/catalyst/admin/operator-dashboard";
import {
  loadOperatorDashboard,
  emptyDashboard,
  DEMO_OWNER,
} from "@data/lib/catalyst/admin/operator-dashboard.server";
import { resolveAssignment, type Assignment } from "@core/lib/experiments/assign";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { parseStory } from "@core/lib/experiments/context";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { track, trackExposure } from "@core/lib/telemetry/track";

import type { Route } from "./+types/operator.dashboard";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "admin/operator-dashboard";
const STORY_DIR = path.join(process.cwd(), "packages", "features", "src", "stories", STORY);

/**
 * The place list here is a PUBLIC read, and the page now says so.
 *
 * `GET /places/api/places?owner=` is unauthenticated:
 * `catalyrst-places/src/handlers/places.rs:66-73` (`get_place_list`) calls
 * `crate::auth::auth_address_optional` and gates nothing. Anyone gets the same
 * answer for any address.
 *
 * `?owner=` is therefore kept -- it is a legitimate filter over public data --
 * but demoted and relabelled. It is not an identity claim and it grants
 * nothing, so the page renders "viewing places for <address>", flags the
 * built-in `DEMO_OWNER` fallback as "demo address, not you", and does not
 * describe any of this as privileged. Every privileged control reachable from
 * here (scene admins, scene bans) is unavailable on this node regardless of the
 * value -- see `control-availability.ts`.
 *
 * A failed read is reported as a reason, not as an empty dashboard: the two
 * used to render nearly identically.
 */
export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const range = coerceRange(url.searchParams.get("range"));
  const ownerParam = url.searchParams.get("owner") ?? readWallet(request);

  const { sid, wrap } = sidLoader(request);

  let assignment: Assignment = {
    variant: "with-dashboard",
    flags: { showOperatorDashboard: true },
    experimentKey: "operator_dashboard",
  };
  try {
    const story = parseStory(STORY_DIR);
    assignment = await resolveAssignment(request, story).catch(() => ({
      variant: story.experiment.variants[0].id,
      flags: story.experiment.variants[0].flags,
      experimentKey: story.experiment.key,
    }));
  } catch {
  }

  const loaded = await loadOperatorDashboard(
    ownerParam ?? DEMO_OWNER,
    { signal: request.signal },
  ).catch((err: unknown) => ({
    dashboard: emptyDashboard(ownerParam ?? DEMO_OWNER),
    viewedAddress: ownerParam ?? DEMO_OWNER,
    isDemo: (ownerParam ?? DEMO_OWNER) === DEMO_OWNER,
    provenance: "public" as const,
    places: {
      ok: false as const,
      reason: "backend-error" as const,
      status: 502,
      message: `Public places list unavailable: ${
        (err as Error)?.message ?? "network error"
      }`,
      serverCheck:
        "catalyrst-places/src/handlers/places.rs:66-73 (auth_address_optional, no gate)",
    },
  }));

  const payload = {
    sid,
    range,
    assignment,
    dashboard: loaded.dashboard,
    viewedAddress: loaded.viewedAddress,
    isDemo: loaded.isDemo,
    unavailableReason: loaded.places.ok ? null : loaded.places.message,
  };

  return wrap(payload);
}

export default function OperatorDashboardRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const [, setSearchParams] = useSearchParams();
  const t = totals(d.dashboard.places);

  const ctx = {
    sid: d.sid,
    story: STORY,
    variant: d.assignment.variant,
    experimentKey: d.assignment.experimentKey,
  };

  useViewed(ctx, t.placeCount, t.totalLivePlayers);

  function onSelectRange(next: Range) {
    if (next === d.range) return;
    track("operator_dashboard_range_changed", { range: next }, ctx);
    setSearchParams(
      (prev) => {
        const p = new URLSearchParams(prev);
        p.set("range", next);
        return p;
      },
      { preventScrollReset: true },
    );
  }

  function onOpen(placeId: string) {
    track("operator_place_card_clicked", { place_id: placeId }, ctx);
  }

  function onModerationLink(placeId: string, target: "scene-bans" | "scene-admins") {
    track(
      "operator_dashboard_moderation_link",
      { place_id: placeId, target },
      ctx,
    );
  }

  return (
    <OpDashboardPage
      range={d.range}
      dashboard={d.dashboard}
      viewedAddress={d.viewedAddress}
      isDemo={d.isDemo}
      unavailableReason={d.unavailableReason}
      LinkComponent={Link}
      onSelectRange={onSelectRange}
      onOpenPlace={onOpen}
      onModerationLink={onModerationLink}
    />
  );
}

type Ctx = {
  sid: string;
  story: string;
  variant: string;
  experimentKey: string;
};

function useViewed(ctx: Ctx, placeCount: number, totalLivePlayers: number) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    trackExposure(ctx);
    track(
      "operator_dashboard_viewed",
      { place_count: placeCount, total_live_players: totalLivePlayers },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
