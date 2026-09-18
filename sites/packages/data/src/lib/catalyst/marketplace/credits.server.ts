import { fetchSeasons, type SeasonsData } from "./credits";
import { ttlMemo } from "../../ttl-memo";

const SEASONS_TTL_MS = 60_000;

const seasonsMemo = ttlMemo({
  ttlMs: SEASONS_TTL_MS,
  keep: (v: SeasonsData | null) => v !== null,
  load: () => fetchSeasons().catch(() => null),
});

export function loadSeasons(): Promise<SeasonsData | null> {
  return seasonsMemo();
}

export function resetSeasonsCache(): void {
  seasonsMemo.reset();
}
