import { getJSON, worldsBase, type GetOptions } from "../client";

export type WorldScene = {
  entityId: string;
  parcels: string[];
  baseParcel: string;
};

/**
 * null when the world's scene list could not be read.
 *
 * `[]` is the answer for a world with nothing deployed, and both callers act on
 * it: `creator-hub.world-permissions` uses `scenes.length` to decide a world
 * does not exist, and `creator-hub.world-settings` renders the unpublish list
 * from it. A failed read must not collapse to `[]` -- that would read as
 * "this world does not exist" on one page and "there is nothing to unpublish"
 * on the other.
 */
export async function loadWorldScenes(
  worldName: string,
  opts: GetOptions = {},
): Promise<WorldScene[] | null> {
  try {
    const raw = await getJSON<{ scenes?: unknown }>(
      `/world/${encodeURIComponent(worldName)}/scenes`,
      { ...opts, base: opts.base ?? worldsBase() },
    );
    const list = Array.isArray(raw?.scenes) ? raw.scenes : [];
    return list
      .map((s): WorldScene => {
        const o = (s ?? {}) as Record<string, unknown>;
        const parcels = Array.isArray(o.parcels)
          ? o.parcels.filter((p): p is string => typeof p === "string")
          : [];
        return {
          entityId: typeof o.entityId === "string" ? o.entityId : "",
          baseParcel: typeof o.baseParcel === "string" ? o.baseParcel : "",
          parcels,
        };
      })
      .filter((s) => s.entityId !== "" && s.baseParcel !== "");
  } catch {
    return null;
  }
}
