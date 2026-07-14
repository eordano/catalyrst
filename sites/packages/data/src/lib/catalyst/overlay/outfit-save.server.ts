import { fetchOutfitSaveData, type OutfitSaveData } from "./outfit-save";

type LoadOpts = {
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
  base?: string;
};

/**
 * null when the profile did not answer.
 *
 * The previous fallback built a whole outfit -- a BaseMale body shape, no
 * wearables, no unlocked name slots -- and labelled it `source: "live"`. The
 * wizard then showed a stranger's avatar as the thing about to be saved, and
 * `namesForExtraSlots: []` locked slots 6-10 for a wallet that may well own a
 * NAME. Both are answers nobody measured, so the read reports its absence and
 * the caller decides what to show.
 */
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
