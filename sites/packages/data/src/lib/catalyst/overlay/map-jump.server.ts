import type { GetOptions } from "../client";
import { loadMapPins, unavailableMapJump, type MapJumpData } from "./map-jump";

/**
 * An empty live list is a real answer ("this node lists no places") and is
 * returned as-is; only a failed read becomes "unavailable". Neither may be
 * replaced with fixture pins -- see `MapJumpData`.
 */
export async function loadMapJump(opts: GetOptions = {}): Promise<MapJumpData> {
  try {
    return await loadMapPins(opts);
  } catch (err) {
    return unavailableMapJump((err as Error)?.message ?? "network error");
  }
}
