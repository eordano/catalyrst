/**
 * Scene admins.
 *
 * Two different things live on this page and they must not be confused:
 *
 * 1. The place list. `GET /places/api/places?owner=0x...` is a **public,
 *    unauthenticated filter** (catalyrst-places/src/handlers/places.rs:66-73,
 *    `auth_address_optional`, no gate). It is fine to call, and it confers no
 *    authority whatsoever. The `owner` value is a filter, not a claim about
 *    who you are -- which is why `viewedAddressIsDemo` exists and why the UI
 *    must say "viewing places for address X (public data)" rather than
 *    "your places".
 *
 * 2. The grants themselves. BLOCK.
 *      list    catalyrst-comms/src/handlers/scene_admin.rs:56-62
 *      grant   catalyrst-comms/src/handlers/scene_admin.rs:123-131
 *      revoke  catalyrst-comms/src/handlers/scene_admin.rs:145-157
 *              -> ports/scene_perms.rs:16-114, denies on pool failure :27-34
 *    The checks are real and good. They are unreachable: there is no nginx
 *    `location` for `/scene-admin`, and the correct public path
 *    (`/comms/scene-admin`) is used nowhere. Adding that edge route is a
 *    deployment change, not part of a UI change.
 *
 * Previously this module returned `grants: []` unconditionally
 * (scene-admins.server.ts:51), which rendered as "this place has no scene
 * admins". It now returns the unavailable reason instead.
 */

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import type { Envelope } from "../schema";
import { parsePlaces, normalizeAddress, type OperatedPlace } from "./scene-admins";
import { controlStatus } from "./control-availability";
import { unavailable, type ControlResult, type Unavailable } from "./availability";

/**
 * A well-known address used when no address is supplied. It is a demo value,
 * not the viewer. Anything rendering it must say so -- hence `isDemo` on the
 * result.
 */
export const DEMO_OWNER = "0x5188e308fee25ac49c10f9fd9270d953c4822ce5";

const PLACES_PUBLIC_CHECK =
  "catalyrst-places/src/handlers/places.rs:66-73 (auth_address_optional, no gate)";

export type SceneAdminsData = {
  /** The address the public place filter was run for. Not an identity claim. */
  viewedAddress: string;
  /** True when `viewedAddress` is the built-in demo address, not the viewer. */
  isDemo: boolean;
  places: ControlResult<OperatedPlace[]>;
  /** Always unavailable on this node. See the module comment. */
  grants: Unavailable;
  selectedPlaceId: string | null;
};

async function fetchPlacesForAddress(
  address: string,
  opts: GetOptions = {},
): Promise<ControlResult<OperatedPlace[]>> {
  try {
    const env = await getJSON<Envelope<unknown[]>>("/places/api/places", {
      ...opts,
      query: { owner: normalizeAddress(address), limit: 100 },
    });
    if (!Array.isArray(env?.data)) {
      return unavailable(
        "backend-error",
        "Public places list unavailable: the response carried no data array",
        { status: 502, serverCheck: PLACES_PUBLIC_CHECK },
      );
    }
    return { ok: true, data: parsePlaces(env.data) };
  } catch (err) {
    return unavailable(
      "backend-error",
      `Public places list unavailable: ${
        (err as Error)?.message ?? "network error"
      }`,
      { status: 502, serverCheck: PLACES_PUBLIC_CHECK },
    );
  }
}

export async function loadSceneAdmins(
  address: string | null | undefined,
  requestedPlaceId: string | null,
  signal?: AbortSignal,
): Promise<SceneAdminsData> {
  const normalized = normalizeAddress(address ?? "") || DEMO_OWNER;
  const isDemo = normalized === DEMO_OWNER;

  const places = await fetchPlacesForAddress(normalized, { signal });
  const rows = places.ok ? places.data : [];

  const selectedPlaceId =
    (requestedPlaceId && rows.some((p) => p.id === requestedPlaceId)
      ? requestedPlaceId
      : rows[0]?.id) ?? null;

  return {
    viewedAddress: normalized,
    isDemo,
    places,
    grants: controlStatus("sceneAdmins.list") as Unavailable,
    selectedPlaceId,
  };
}

export function grantSceneAdmin(): Unavailable {
  return controlStatus("sceneAdmins.grant") as Unavailable;
}

export function revokeSceneAdmin(): Unavailable {
  return controlStatus("sceneAdmins.revoke") as Unavailable;
}
