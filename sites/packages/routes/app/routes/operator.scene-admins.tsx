import { useEffect, useRef } from "react";
import { useSearchParams } from "react-router";

import OpSceneAdminsPage from "@ui/operator/pages/OpSceneAdminsPage";

import {
  loadSceneAdmins,
  DEMO_OWNER,
} from "@data/lib/catalyst/admin/scene-admins.server";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track, trackExposure, type TrackContext } from "@core/lib/telemetry/track";

import type { Route } from "./+types/operator.scene-admins";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "admin/operator-scene-admins";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "operator_scene_admins_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const address =
    url.searchParams.get("owner")?.trim() || readWallet(request) || DEMO_OWNER;
  const requestedPlace = url.searchParams.get("place")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
    { skipExposure: true },
  );

  const data = await loadSceneAdmins(address, requestedPlace, request.signal);

  const payload = {
    sid,
    viewedAddress: data.viewedAddress,
    isDemo: data.isDemo,
    places: data.places.ok ? data.places.data : [],
    placesUnavailableReason: data.places.ok ? null : data.places.message,
    selectedPlaceId: data.selectedPlaceId,
    grants: {
      message: data.grants.message,
      serverCheck: data.grants.serverCheck,
      fix: data.grants.fix,
    },
    assignment,
  };

  return wrap(payload);
}

export default function OperatorSceneAdminsRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const [, setSearchParams] = useSearchParams();

  const ctx: TrackContext = {
    sid: d.sid,
    story: STORY,
    variant: d.assignment.variant,
    experimentKey: d.assignment.experimentKey,
  };

  useUnavailableViewed(ctx, d.grants.message);

  function onSelectPlace(placeId: string) {
    setSearchParams(
      (prev) => {
        const p = new URLSearchParams(prev);
        p.set("place", placeId);
        return p;
      },
      { preventScrollReset: true },
    );
  }

  return (
    <OpSceneAdminsPage
      viewedAddress={d.viewedAddress}
      isDemo={d.isDemo}
      places={d.places}
      placesUnavailableReason={d.placesUnavailableReason}
      selectedPlaceId={d.selectedPlaceId}
      onSelectPlace={onSelectPlace}
      grantsMessage={d.grants.message}
      grantsServerCheck={d.grants.serverCheck}
      grantsFix={d.grants.fix}
    />
  );
}

function useUnavailableViewed(ctx: TrackContext, reason: string) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    trackExposure(ctx);
    track(
      "operator_control_unavailable",
      { control: "sceneAdmins.list", reason },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
