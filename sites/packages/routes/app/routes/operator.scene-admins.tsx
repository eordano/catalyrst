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

/**
 * Scene admins -- BLOCK for the grants, public read for the place list.
 *
 * The grant/list/revoke checks in `catalyrst-comms` are real and correct
 * (`handlers/scene_admin.rs:56-62`, `:123-131`, `:145-157` ->
 * `ports/scene_perms.rs:16-114`, denying on pool failure at `:27-34`), but they
 * are not reachable from this node: nginx has no `location` for
 * `/scene-admin`, and the correct public path `/comms/scene-admin` is used
 * nowhere. Adding that edge route is a deployment change and is deliberately
 * not part of this UI change.
 *
 * So the wizard is not rendered at all. `scene-admins.server.ts` used to
 * hardcode `grants: []`, which displayed as "this place has no scene admins"
 * next to Add and Revoke buttons that could never work. The page now shows the
 * server-side check it is subject to, why it cannot be reached, and disabled
 * controls carrying that reason.
 *
 * `?owner=` survives as what it actually is: a filter over the public places
 * list (`catalyrst-places/src/handlers/places.rs:66-73`, `auth_address_optional`,
 * no gate). It is labelled as such, and the `DEMO_OWNER` fallback is surfaced
 * as a demo address rather than as the viewer.
 */
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
