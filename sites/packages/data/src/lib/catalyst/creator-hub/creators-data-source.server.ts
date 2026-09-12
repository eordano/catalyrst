import { getJSON } from "../client";
import { creatorsDataBase } from "./scene-analytics";
import {
  classifyMetricsArtifact,
  worldMetricsPath,
} from "./creators-data-source";
import {
  endpointLabel,
  snapshotFrom,
  unavailableBecause,
  unavailableFrom,
  type Datum,
} from "./datum.server";

export type CreatorsDataOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
};

export async function loadWorldMetricsArtifact(
  world: string,
  opts: CreatorsDataOptions = {},
): Promise<Datum<unknown>> {
  const base = creatorsDataBase(opts.base);
  const path = worldMetricsPath(world);
  const endpoint = endpointLabel("GET", `${base}${path}`);
  let raw: unknown;
  try {
    raw = await getJSON<unknown>(path, {
      base,
      signal: opts.signal,
      fetchImpl: opts.fetchImpl,
    });
  } catch (err) {
    return unavailableFrom(
      err,
      endpoint,
      "decentraland.org/creators-data serves the marketing SPA; the metrics service is not deployed.",
    );
  }

  const verdict = classifyMetricsArtifact(raw);
  if (verdict.kind === "unavailable") {
    return unavailableBecause(endpoint, `${endpoint} answered. ${verdict.reason}`);
  }
  return snapshotFrom(
    verdict.value,
    endpoint,
    verdict.exportedAt,
    verdict.exportSource,
  );
}
