/**
 * Scene bans -- BLOCK, and the fixture masking is removed.
 *
 * Server-side checks (read, correct, and unreachable from this node):
 *   list   catalyrst-comms/src/handlers/scene_bans.rs:88-89   verify_signed_fetch
 *   ban    catalyrst-comms/src/handlers/scene_bans.rs:170-178 -> ports/scene_perms.rs:16-114
 *   unban  catalyrst-comms/src/handlers/scene_bans.rs:193-205 -> ports/scene_perms.rs:16-114
 *
 * `scene_perms.rs:27-34` denies on pool failure, so the check itself fails
 * closed. The problem is reachability: there is no nginx `location` for
 * `/scene-admin`, and the correct public path (`/comms/scene-admin`) is used
 * nowhere. Adding that edge route is a deployment config change and is
 * explicitly not part of a UI change.
 *
 * What was here before: `loadSceneBansPage` called the live endpoint, caught
 * the 401 with a bare `catch {}`, and returned fixture rows tagged
 * `source: "fixture"`. A moderator looking at that page saw plausible ban rows
 * for a scene, produced by a JSON file. That is removed. The page now gets an
 * unavailable result with the reason.
 *
 * The fixture is NOT deleted -- it is still the layout source for the story and
 * for `loadOperatorPlaces` below -- but it can no longer stand in for a live
 * answer, and everything it returns is tagged `synthetic: true`.
 */

import fs from "node:fs";
import path from "node:path";

import { controlStatus } from "./control-availability";
import type { Unavailable } from "./availability";
import { PlaceRefSchema, type PlaceRef } from "./scene-bans";

const FIXTURE = path.join(
  process.cwd(),
  "packages",
  "data",
  "src",
  "fixtures",
  "operator-scene-bans.json",
);

type FixtureShape = {
  default_owner?: string;
  places?: unknown[];
  bans?: Record<string, unknown>;
};

/** `null` when the bundled file could not be read or parsed. */
function readFixture(): FixtureShape | null {
  try {
    return JSON.parse(fs.readFileSync(FIXTURE, "utf8")) as FixtureShape;
  } catch {
    return null;
  }
}

export type OperatorPlacesFixture = {
  places: PlaceRef[];
  owner: string;
  /** Always true. These rows come from a JSON file, not from the network. */
  synthetic: true;
  /**
   * True when the bundled fixture itself could not be read. `places` is then
   * empty because the file is missing from the build, not because the layout
   * data lists no scenes.
   */
  unreadable: boolean;
};

/**
 * Place list for the scene-bans story layout. Synthetic by construction -- the
 * caller must label it as such. The live, public equivalent is
 * `GET /places/api/places?owner=` (catalyrst-places/src/handlers/places.rs:66-73).
 */
export function loadOperatorPlaces(): OperatorPlacesFixture {
  const fx = readFixture();
  const places: PlaceRef[] = [];
  for (const row of fx?.places ?? []) {
    const parsed = PlaceRefSchema.safeParse(row);
    if (parsed.success) places.push(parsed.data);
  }
  return {
    places,
    owner: fx?.default_owner ?? "",
    synthetic: true,
    unreadable: fx === null,
  };
}

/**
 * Reading the ban list for a scene. Always unavailable on this node; see the
 * module comment. No request is made, because the route it would call is not
 * exposed at the edge -- a request here would 404 through the sites catch-all
 * and look like an empty scene.
 */
export function loadSceneBansPage(_placeId: string): Unavailable {
  return controlStatus("sceneBans.list") as Unavailable;
}

export function banInScene(): Unavailable {
  return controlStatus("sceneBans.ban") as Unavailable;
}

export function unbanInScene(): Unavailable {
  return controlStatus("sceneBans.unban") as Unavailable;
}
