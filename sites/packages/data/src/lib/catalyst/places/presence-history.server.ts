import { buildQuery, catalystBase } from "../client";
import type { GetOptions } from "../client";
import {
  bucketize,
  clampHistoryLimit,
  fetchSceneHistory,
  fetchWorldHistory,
  sceneHistoryPath,
  worldHistoryPath,
  type BucketizedHistory,
} from "./presence-history";
import {
  DEFAULT_CADENCE_SECONDS,
  endpointLabel,
  noSample,
  sampledAt,
  unavailableFrom,
  type Datum,
} from "../creator-hub/datum.server";

export type HistoryOptions = GetOptions & { cadenceSeconds?: number };

function label(path: string, query: Record<string, string | number>): string {
  return endpointLabel("GET", `${catalystBase()}${path}${buildQuery(query)}`);
}

/**
 * A history read that succeeds but returns no rows is NOT a zero series and is
 * not a failure either: the collector simply has no snapshots for this subject,
 * because it has not been in the poll set. That is `no-sample`, with the reason
 * spelled out, so the screen renders an empty state rather than a flat line.
 */
export async function loadWorldOccupancyHistory(
  world: string,
  limit: number,
  opts: HistoryOptions = {},
): Promise<Datum<BucketizedHistory>> {
  const clamped = clampHistoryLimit(limit);
  const endpoint = label(worldHistoryPath(), { world, limit: clamped });
  const cadence = opts.cadenceSeconds ?? DEFAULT_CADENCE_SECONDS;
  try {
    const rows = await fetchWorldHistory(world, clamped, opts);
    const history = bucketize(rows, cadence);
    if (history.lastSeen === null) {
      return noSample(
        endpoint,
        new Date().toISOString(),
        `No occupancy recorded for ${world}. The presence collector has no snapshots for it \u{2014} that means it has not been in the poll set, not that it had no visitors.`,
      );
    }
    return sampledAt(history, endpoint, history.lastSeen, cadence);
  } catch (err) {
    return unavailableFrom(
      err,
      endpoint,
      "The occupancy chart is not shown; the right-now counts above come from a different read.",
    );
  }
}

/**
 * The Genesis-parcel escape hatch: occupancy history for one pointer. There is
 * no way to list a wallet's parcels -- presence keys occupancy by pointer with
 * no owner field -- so a pointer can only ever be looked up.
 */
export async function loadSceneOccupancyHistory(
  pointer: string,
  limit: number,
  opts: HistoryOptions = {},
): Promise<Datum<BucketizedHistory>> {
  const clamped = clampHistoryLimit(limit);
  const endpoint = label(sceneHistoryPath(), { pointer, limit: clamped });
  const cadence = opts.cadenceSeconds ?? DEFAULT_CADENCE_SECONDS;
  try {
    const rows = await fetchSceneHistory(pointer, clamped, opts);
    const history = bucketize(rows, cadence);
    if (history.lastSeen === null) {
      return noSample(
        endpoint,
        new Date().toISOString(),
        `No occupancy recorded at ${pointer}. The presence collector has no snapshots for that parcel \u{2014} it has not been in the poll set, which is not the same as nobody visiting it.`,
      );
    }
    return sampledAt(history, endpoint, history.lastSeen, cadence);
  } catch (err) {
    return unavailableFrom(err, endpoint);
  }
}
