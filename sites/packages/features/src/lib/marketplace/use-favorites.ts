import { useEffect, useMemo, useState } from "react";

import type { ShopCard } from "@ui/marketplace/new-shop/NewShopHome";

import { openSignIn } from "../../components/auth/signin-store";
import { getIdentity } from "@data/lib/auth/session";
import { track, type TrackContext } from "@core/lib/telemetry/track";
import { getFavorites, subscribe as subscribeFavorites, toggleFavorite } from "./favorites";
import { syncFavorite, type FavoriteSyncResult } from "./favorites-sync";

export function useFavorites(trackCtx: TrackContext) {
  const [favCards, setFavCards] = useState<ShopCard[]>([]);
  const [syncState, setSyncState] = useState<FavoriteSyncResult | null>(null);

  useEffect(() => {
    const sync = () => setFavCards(getFavorites());
    sync();
    return subscribeFavorites(sync);
  }, []);

  const favIds = useMemo(() => favCards.map((c) => c.id), [favCards]);

  function toggle(card: ShopCard): boolean {
    const identity = getIdentity();
    track(
      "mk_favorite_toggle",
      { item_id: card.id, signed_in: !!identity },
      trackCtx,
    );
    if (!identity) {
      openSignIn();
      return false;
    }
    const favorited = toggleFavorite(card);
    void syncFavorite(identity, card.id, favorited).then((result) => {
      setSyncState(result);
      if (result !== "synced") {
        track(
          result === "failed" ? "mk_favorite_sync_failed" : "mk_favorite_sync_unavailable",
          { item_id: card.id, on: favorited },
          trackCtx,
        );
      }
    });
    return true;
  }

  return { favCards, favIds, toggle, syncState };
}
