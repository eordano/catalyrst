import { useEffect, useRef } from "react";
import { useSearchParams } from "react-router";

import AdControlNotice, { AdBlockedAction } from "@ui/admin/pages/AdControlNotice";
import SitesChrome from "@ui/web/frames/SitesChrome";
import OpPlacePicker from "@ui/operator/components/OpPlacePicker";

import {
  loadOperatorPlaces,
  loadSceneBansPage,
} from "@data/lib/catalyst/admin/scene-bans.server";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track, type TrackContext } from "@core/lib/telemetry/track";

import type { Route } from "./+types/operator.scene-bans";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "admin/operator-scene-bans";
const EXPERIMENT_KEY = "operator_scene_bans";

const FALLBACK: Assignment = {
  variant: "list",
  flags: { confirmStep: true },
  experimentKey: EXPERIMENT_KEY,
};

/**
 * Scene bans -- BLOCK, and the fixture no longer stands in for a live answer.
 *
 * The server-side checks are real and fail closed:
 *   list   `catalyrst-comms/src/handlers/scene_bans.rs:88-89` `verify_signed_fetch`
 *   ban    `catalyrst-comms/src/handlers/scene_bans.rs:170-178`
 *   unban  `catalyrst-comms/src/handlers/scene_bans.rs:193-205`
 *          -> `ports/scene_perms.rs:16-114`, denying on pool failure (`:27-34`)
 *
 * They are unreachable from this node for the same reason as scene admins:
 * nginx has no `location` for `/scene-admin`, and `/comms/scene-admin` is used
 * nowhere. That is a deployment change, not a UI change.
 *
 * What was here before: the loader called the live endpoint, a bare `catch {}`
 * swallowed the 401, and `src/fixtures/operator-scene-bans.json` supplied ban
 * rows tagged `source: "fixture"`. A moderator saw plausible bans for a scene,
 * produced by a JSON file, above a working-looking ban form. Both are gone.
 *
 * The fixture is not deleted -- it is still the place list for this layout -- but
 * `loadOperatorPlaces()` now returns `synthetic: true` and the page says so.
 */
export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const {
    places,
    owner: fixtureOwner,
    synthetic,
    unreadable: placesUnreadable,
  } = loadOperatorPlaces();
  const viewedAddress =
    url.searchParams.get("owner")?.trim() || readWallet(request) || fixtureOwner;

  const requested = url.searchParams.get("place")?.trim() || "";
  const selectedPlaceId =
    (requested && places.some((p) => p.id === requested)
      ? requested
      : places[0]?.id) ?? null;

  const bans = loadSceneBansPage(selectedPlaceId ?? "");

  const payload = {
    sid,
    viewedAddress,
    places,
    placesAreSynthetic: synthetic,
    placesUnreadable,
    selectedPlaceId,
    bans: {
      message: bans.message,
      serverCheck: bans.serverCheck,
      fix: bans.fix,
    },
    assignment,
  };

  return wrap(payload);
}

export default function OperatorSceneBans({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const [, setSearchParams] = useSearchParams();

  const ctx: TrackContext = {
    sid: d.sid,
    story: STORY,
    variant: d.assignment.variant,
    experimentKey: d.assignment.experimentKey,
  };

  useUnavailableViewed(ctx, d.bans.message);

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
    <SitesChrome active="create">
      <main className="sa-route">
        <div className="sa__head">
          <h1 className="sa__title">Scene bans</h1>
          <p className="sa__sub">
            Viewing <code>{d.viewedAddress}</code>. Banning and unbanning in a
            scene are not available on this node.
          </p>
        </div>

        {d.placesUnreadable && (
          <AdControlNotice
            tone="sample"
            title="Place list could not be read"
            message={
              "src/fixtures/operator-scene-bans.json is missing from this build, " +
              "so the picker below is empty because nothing was read \u{2014} not " +
              "because there are no places."
            }
          />
        )}

        {d.placesAreSynthetic && !d.placesUnreadable && (
          <AdControlNotice
            tone="sample"
            title="Sample place list"
            message={
              "These places come from src/fixtures/operator-scene-bans.json, not " +
              "from the network. They are layout data, not a list of scenes you " +
              "can moderate."
            }
          />
        )}

        <OpPlacePicker
          places={d.places.map((p) => ({
            id: p.id,
            title: p.title,
            base_position: p.base_position,
            image: p.image,
            user_count: p.user_count,
          }))}
          selectedId={d.selectedPlaceId}
          onSelect={onSelectPlace}
          owner={d.viewedAddress}
        />

        <AdControlNotice
          title="Scene ban list"
          message={d.bans.message}
          serverCheck={d.bans.serverCheck}
          fix={d.bans.fix}
        />

        <div className="sa__toolbar">
          <AdBlockedAction label="Ban in this scene" reason={d.bans.message} />
          <AdBlockedAction label="Unban" reason={d.bans.message} />
        </div>
      </main>
    </SitesChrome>
  );
}

function useUnavailableViewed(ctx: TrackContext, reason: string) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(
      "operator_control_unavailable",
      { control: "sceneBans.list", reason },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
