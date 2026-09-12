import type { GetOptions } from "../client";
import { loadMapPins, unavailableMapJump, type MapJumpData } from "./map-jump";

export async function loadMapJump(opts: GetOptions = {}): Promise<MapJumpData> {
  try {
    return await loadMapPins(opts);
  } catch (err) {
    return unavailableMapJump((err as Error)?.message ?? "network error");
  }
}
