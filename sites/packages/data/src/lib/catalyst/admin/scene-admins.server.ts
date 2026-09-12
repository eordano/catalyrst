
import { getJSON } from "../client";
import type { GetOptions } from "../client";
import type { Envelope } from "../schema";
import { parsePlaces, normalizeAddress, type OperatedPlace } from "./scene-admins";
import { controlStatus } from "./control-availability";
import { unavailable, type ControlResult, type Unavailable } from "./availability";

export const DEMO_OWNER = "0x5188e308fee25ac49c10f9fd9270d953c4822ce5";

const PLACES_PUBLIC_CHECK =
  "catalyrst-places/src/handlers/places.rs:66-73 (auth_address_optional, no gate)";

export type SceneAdminsData = {
  viewedAddress: string;
  isDemo: boolean;
  places: ControlResult<OperatedPlace[]>;
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
