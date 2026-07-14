import { fetchPacks, type Pack } from "./packs";
import type { GetOptions } from "../client";

/**
 * "live" -- the node answered and these are the packs it sells.
 * "empty" -- the node answered and sells no packs.
 * "unavailable" -- the read failed; `data` is empty because we know nothing, not
 *   because nothing is for sale. This is a purchase page: rendering the two the
 *   same way tells a paying customer the product does not exist.
 */
export type PacksLoad = {
  data: Pack[];
  isFixture: boolean;
  source: "live" | "empty" | "unavailable";
  reason?: string;
};

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function loadPacks(opts: GetOptions = {}): Promise<PacksLoad> {
  try {
    const data = await fetchPacks(opts);
    return {
      data,
      isFixture: false,
      source: data.length ? "live" : "empty",
    };
  } catch (error) {
    return {
      data: [],
      isFixture: false,
      source: "unavailable",
      reason: message(error),
    };
  }
}
