import { fetchSeasons, type SeasonsData } from "./credits";

export async function loadSeasons(
  signal?: AbortSignal,
): Promise<SeasonsData | null> {
  return await fetchSeasons({ signal }).catch(() => null);
}
