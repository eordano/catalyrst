/**
 * Operator dashboard place list -- a PUBLIC read.
 *
 * Server side: catalyrst-places/src/handlers/places.rs:66-73 `get_place_list`
 * calls `crate::auth::auth_address_optional` and gates nothing. `?owner=` is a
 * filter over public data. Anyone, signed in or not, gets the same answer.
 *
 * So this page must not present itself as privileged. The address in the URL
 * (or the cookie, or `DEMO_OWNER`) selects *whose places to display*; it does
 * not assert who the viewer is and grants nothing. `isDemo` exists so the UI
 * can say "demo address -- not you" instead of implying ownership.
 *
 * Every privileged control reachable from this dashboard (scene admins, scene
 * bans) is unavailable regardless of this value -- see `control-availability.ts`.
 */

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import type { Envelope } from "../schema";
import {
  OperatorPlaceSchema,
  type OperatorDashboard,
  type OperatorPlace,
} from "./operator-dashboard";
import { normalizeAddress } from "./scene-admins";
import { unavailable, type ControlResult } from "./availability";

export const DEMO_OWNER = "0x5188e308fee25ac49c10f9fd9270d953c4822ce5";

const PLACES_PUBLIC_CHECK =
  "catalyrst-places/src/handlers/places.rs:66-73 (auth_address_optional, no gate)";

export type LoadResult = {
  dashboard: OperatorDashboard;
  /** The address the public filter was run for. Not an identity claim. */
  viewedAddress: string;
  /** True when the address is the built-in demo value, not the viewer. */
  isDemo: boolean;
  /** Provenance label the UI must render. */
  provenance: "public";
  places: ControlResult<OperatorPlace[]>;
};

export function emptyDashboard(owner: string): OperatorDashboard {
  return {
    owner: normalizeAddress(owner) || owner,
    owner_name: null,
    snapshot_taken_at: null,
    snapshot_interval_min: null,
    places: [],
  };
}

async function fetchPlacesForAddress(
  address: string,
  opts: GetOptions,
): Promise<ControlResult<OperatorPlace[]>> {
  try {
    const env = await getJSON<Envelope<unknown[]>>("/places/api/places", {
      ...opts,
      query: { owner: address, limit: 100 },
    });
    if (!Array.isArray(env?.data)) {
      return unavailable(
        "backend-error",
        "Public places list unavailable: the response carried no data array",
        { status: 502, serverCheck: PLACES_PUBLIC_CHECK },
      );
    }
    const out: OperatorPlace[] = [];
    for (const row of env.data) {
      const parsed = OperatorPlaceSchema.safeParse(row);
      if (parsed.success) out.push(parsed.data);
    }
    return { ok: true, data: out };
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

export async function loadOperatorDashboard(
  address: string = DEMO_OWNER,
  opts: GetOptions = {},
): Promise<LoadResult> {
  const normalized = normalizeAddress(address) || DEMO_OWNER;
  const places = await fetchPlacesForAddress(normalized, opts);

  return {
    dashboard: {
      owner: normalized,
      owner_name: null,
      snapshot_taken_at: null,
      snapshot_interval_min: null,
      places: places.ok ? places.data : [],
    },
    viewedAddress: normalized,
    isDemo: normalized === DEMO_OWNER,
    provenance: "public",
    places,
  };
}
