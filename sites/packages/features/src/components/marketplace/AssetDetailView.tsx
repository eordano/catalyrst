import { useEffect, useRef, useState, type ComponentProps } from "react";
import { useNavigate } from "react-router";

import MkAssetDetailView, {
  CREDITS_UNAVAILABLE_NOTE,
  PRIMARY_MINT_LABEL,
  PRIMARY_MINT_NOTE,
} from "@ui/marketplace/pages/MkAssetDetailView";

type MkAssetNftProp = ComponentProps<typeof MkAssetDetailView>["nft"];

import TryOnPreview, { canTryOn } from "./TryOnPreview";
import { getIdentity } from "@data/lib/auth/session";
import {
  addCartItem,
  fetchCart,
  parseItemRef,
} from "@data/lib/catalyst/marketplace/cart";
import type { AssetDetail } from "@data/lib/catalyst/marketplace/index";
import type { AssetListing } from "@data/lib/catalyst/marketplace/orders";
import { useAuth } from "@data/lib/auth/index";
import { useFavorites } from "../../lib/marketplace/use-favorites";
import { track } from "@core/lib/telemetry/track";
import type { TrackContext } from "@core/lib/telemetry/track";
import { openSignIn } from "../auth/signin-store";

export type AssetDetailViewProps = {
  itemId: string;
  nft: AssetDetail;
  listings: AssetListing[];
  quoteOk: boolean;
  prominentPrice: boolean;
  trackCtx: TrackContext;
};

export default function AssetDetailView({
  itemId,
  nft,
  listings,
  quoteOk,
  prominentPrice,
  trackCtx,
}: AssetDetailViewProps) {
  const navigate = useNavigate();
  const auth = useAuth();
  const [error, setError] = useState<string | null>(null);
  const [cartState, setCartState] = useState<"idle" | "adding" | "added">(
    "idle",
  );
  const pendingAdd = useRef(false);
  const [cartAnnounce, setCartAnnounce] = useState("");

  useEffect(() => {
    const identity = getIdentity();
    const ref = parseItemRef(itemId);
    if (!identity || !ref) return;
    let cancelled = false;
    fetchCart(identity)
      .then((cart) => {
        if (cancelled) return;
        const inCart = cart.items.some((l) => {
          if (l.itemId !== ref.itemId) return false;
          if (l.collection) return l.collection.toLowerCase() === ref.collection;
          return l.urn.toLowerCase().includes(ref.collection);
        });
        if (inCart) setCartState((s) => (s === "idle" ? "added" : s));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [itemId]);

  const order = nft.order;
  const hasOpenListing = listings.some((l) => l.listed);
  const notFulfillable = !!order && quoteOk && order.credits == null;
  const primaryMintOnly =
    notFulfillable && !hasOpenListing && order?.source === "mint";
  const buyDisabledReason = notFulfillable
    ? primaryMintOnly
      ? PRIMARY_MINT_NOTE
      : CREDITS_UNAVAILABLE_NOTE
    : undefined;

  const displayNft: AssetDetail =
    primaryMintOnly && order
      ? { ...nft, order: { ...order, expiresLabel: PRIMARY_MINT_LABEL } }
      : nft;
  const displayOrder = displayNft.order;
  const price = displayOrder?.price ?? null;
  const credits = displayOrder?.credits ?? null;

  const { favIds, toggle: toggleFav } = useFavorites(trackCtx);
  const favorited = favIds.includes(itemId);
  const favCard = {
    id: itemId,
    name: nft.name,
    meta: nft.collection.name || "Collectible",
    price:
      credits ?? (price != null ? Number(price).toLocaleString() : undefined),
    unit: (credits != null ? "credits" : "mana") as "credits" | "mana",
    rarity: nft.rarity,
    network: nft.network,
    image: nft.image,
  };

  function onBuy() {
    track("mk_buy_clicked", { item_id: itemId }, trackCtx);
    const ref = parseItemRef(itemId);
    if (!ref) {
      setError("This item isn't listed for direct purchase \u{2014} try making an offer.");
      return;
    }
    setError(null);
    const dest = `/marketplace/checkout?express=${encodeURIComponent(itemId)}`;
    if (!getIdentity()) {
      openSignIn({ redirectTo: dest });
      return;
    }
    navigate(dest);
  }

  async function onAddToCart() {
    if (cartState === "added") {
      navigate("/marketplace/cart");
      return;
    }
    if (cartState === "adding") return;
    track("mk_add_to_cart", { item_id: itemId }, trackCtx);
    const ref = parseItemRef(itemId);
    if (!ref) {
      setError("This item can't be added to the cart.");
      return;
    }
    if (!getIdentity()) {
      pendingAdd.current = true;
      openSignIn();
      return;
    }
    setError(null);
    setCartState("adding");
    setCartAnnounce("Adding to cart");
    try {
      await addCartItem(getIdentity()!, ref, 1);
      setCartState("added");
      setCartAnnounce("Added to cart");
    } catch (err) {
      setCartState("idle");
      setCartAnnounce("");
      setError((err as Error)?.message ?? "Could not add the item to the cart.");
    }
  }

  useEffect(() => {
    if (!auth.isConnected || !pendingAdd.current) return;
    pendingAdd.current = false;
    void onAddToCart();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auth.isConnected]);

  function onMakeOffer() {
    track("mk_make_offer_clicked", { item_id: itemId }, trackCtx);
    navigate(`/marketplace/bid?id=${encodeURIComponent(itemId)}`);
  }

  function onViewListing(index: number) {
    const listing = listings[index];
    if (!listing?.tokenId || !listing.listed) return;
    track(
      "mk_view_listing_clicked",
      { item_id: itemId, token_id: listing.tokenId },
      trackCtx,
    );
    navigate(
      `/marketplace/buy?nft=${encodeURIComponent(
        `${listing.contractAddress}-${listing.tokenId}`,
      )}`,
    );
  }

  // The table cell is typed `string`; a listing whose wei price could not be
  // read says so rather than borrowing the "0" that renders as Free.
  const listingRows = listings.map((l) => ({ ...l, price: l.price ?? "unknown" }));

  return (
    <MkAssetDetailView
      nft={displayNft as unknown as MkAssetNftProp}
      listings={listingRows}
      cartAnnounce={cartAnnounce}
      error={error}
      primaryMintOnly={primaryMintOnly}
      prominentPrice={prominentPrice}
      hasOrder={!!displayOrder}
      price={price}
      credits={credits}
      buyDisabledReason={buyDisabledReason}
      favorited={favorited}
      tryOn={
        canTryOn(nft.network, nft.category, nft.kind) ? (
          <TryOnPreview itemId={itemId} network={nft.network} />
        ) : undefined
      }
      addToCartLabel={
        cartState === "adding"
          ? "Adding\u{2026}"
          : cartState === "added"
            ? "In cart \u{2014} view"
            : undefined
      }
      onToggleFavorite={() => toggleFav(favCard)}
      onBuy={onBuy}
      onAddToCart={
        credits != null && !buyDisabledReason ? onAddToCart : undefined
      }
      onMakeOffer={!order || buyDisabledReason ? onMakeOffer : undefined}
      onViewListing={onViewListing}
      onBack={() => navigate("/shop")}
    />
  );
}
