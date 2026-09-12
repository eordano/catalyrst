import { fetchOutfitSaveData, type OutfitSaveData } from "./outfit-save";

type LoadOpts = {
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
  base?: string;
};

export async function loadOutfitSaveData(
  address: string,
  opts: LoadOpts = {},
): Promise<OutfitSaveData | null> {
  try {
    return await fetchOutfitSaveData(address, opts);
  } catch {
    return null;
  }
}
