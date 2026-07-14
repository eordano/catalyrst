import { PickUnpickEnvelopeSchema } from "@data/lib/catalyst/generated-schemas/market";

import type { AuthIdentity } from "@data/lib/auth/types";
import { CatalystError, postJSON } from "@data/lib/catalyst/client";

export type FavoriteSyncResult = "synced" | "unavailable" | "failed";

export async function syncFavorite(
  identity: AuthIdentity,
  itemId: string,
  favorited: boolean,
): Promise<FavoriteSyncResult> {
  const path = `/market/v1/picks/${encodeURIComponent(itemId)}`;
  try {
    const res = favorited
      ? await postJSON<unknown>(path, {}, { identity })
      : await postJSON<unknown>(path, undefined, { identity, method: "DELETE" });
    return PickUnpickEnvelopeSchema.safeParse(res).success ? "synced" : "failed";
  } catch (err) {
    if (
      err instanceof CatalystError &&
      (err.status === 404 || err.status === 405 || err.status === 501)
    ) {
      return "unavailable";
    }
    return "failed";
  }
}
