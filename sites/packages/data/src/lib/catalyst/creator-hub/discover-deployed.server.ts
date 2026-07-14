import { catalystBase } from "../client";
import {
  SCENE_CAP,
  normalizeDeployments,
  type DiscoveredScene,
  type RawDeployment,
} from "./discover-deployed";

type DeploymentsPage = {
  deployments?: RawDeployment[];
  pagination?: { next?: string | null };
};

const MAX_PAGES = 30;

function initialParams(wallet: string): URLSearchParams {
  const p = new URLSearchParams();
  p.set("deployedBy", wallet);
  p.set("entityTypes", "scene");
  p.set("onlyCurrentlyPointed", "true");
  p.set("limit", "100");
  return p;
}

function nextParams(next: string): URLSearchParams {
  const qs = next.startsWith("?") ? next.slice(1) : next;
  const p = new URLSearchParams(qs);
  if (!p.has("entityTypes")) p.set("entityTypes", "scene");
  return p;
}

export async function discoverDeployedScenes(
  address: string,
  opts: { base?: string; signal?: AbortSignal } = {},
): Promise<DiscoveredScene[]> {
  const wallet = (address ?? "").trim().toLowerCase();
  if (!wallet) return [];

  const base = catalystBase(opts.base);
  const collected: RawDeployment[] = [];
  let params: URLSearchParams | null = initialParams(wallet);
  let pages = 0;

  try {
    while (params && pages < MAX_PAGES && collected.length < SCENE_CAP * 3) {
      pages += 1;
      const url = `${base}/content/deployments?${params.toString()}`;
      const res = await fetch(url, {
        signal: opts.signal,
        headers: { accept: "application/json" },
      });
      if (!res.ok) break;

      const body = (await res.json()) as DeploymentsPage;
      const deployments = Array.isArray(body?.deployments) ? body.deployments : [];
      collected.push(...deployments);

      const next = body?.pagination?.next;
      if (deployments.length === 0 || typeof next !== "string" || !next) {
        params = null;
      } else {
        params = nextParams(next);
      }
    }
  } catch (err) {
    console.warn(
      "[catalyst] discoverDeployedScenes failed",
      (err as Error)?.message ?? err,
    );
  }

  return normalizeDeployments(collected, base).slice(0, SCENE_CAP);
}
