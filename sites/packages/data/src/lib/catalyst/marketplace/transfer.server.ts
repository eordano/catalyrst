import type { GetOptions } from "../client";
import { fetchOwnedAssets, type OwnedElement } from "./transfer";

export type OwnedAssetsResult = {
  owner: string;
  elements: OwnedElement[] | null;
  source: "catalyst" | "unavailable";
  reason: string | null;
};

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function loadOwnedAssets(
  address: string,
  opts: GetOptions = {},
): Promise<OwnedAssetsResult> {
  try {
    const elements = await fetchOwnedAssets(address, { first: 24 }, opts);
    return { owner: address, elements, source: "catalyst", reason: null };
  } catch (error) {
    return {
      owner: address,
      elements: null,
      source: "unavailable",
      reason: message(error),
    };
  }
}
