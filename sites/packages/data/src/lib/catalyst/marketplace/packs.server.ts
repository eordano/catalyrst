import { fetchPacks, type Pack } from "./packs";
import type { GetOptions } from "../client";

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
