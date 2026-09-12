import { getJSON, worldsBase, type GetOptions } from "../client";

export type WorldScene = {
  entityId: string;
  parcels: string[];
  baseParcel: string;
};

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
