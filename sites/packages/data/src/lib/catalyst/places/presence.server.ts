import type { GetOptions } from "../client";
import { fetchPresenceSnapshot, type PresenceSnapshot } from "./presence";

export type { PresenceSnapshot } from "./presence";

export async function loadPresenceSnapshot(
  opts: GetOptions = {},
): Promise<PresenceSnapshot> {
  try {
    return await fetchPresenceSnapshot(opts);
  } catch {
    return { current: null, scenes: null, worlds: null, source: "unavailable" };
  }
}
