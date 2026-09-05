import type { ComponentProps, ReactNode } from "react";

import { Coin } from "../../atoms/icons";
import { creditsNoun } from "../credits-unit";
import MkAssetPage from "./MkAssetPage";
import "./assetdetailview.css";

export const PRIMARY_MINT_NOTE =
  "Make an offer to buy this item \u{2014} it's a new mint sold by its creator, and Credits checkout doesn't support new mints yet.";
export const CREDITS_UNAVAILABLE_NOTE =
  "Direct checkout isn't available for this item right now \u{2014} use a listing below to buy a specific copy.";
export const PRIMARY_MINT_LABEL = "Sold by its creator as a new mint";

type MkAssetPageProps = ComponentProps<typeof MkAssetPage>;

export type MkAssetDetailViewProps = {
  nft?: MkAssetPageProps["nft"];
  listings?: MkAssetPageProps["listings"];
  cartAnnounce?: string;
  error?: string | null;
  primaryMintOnly?: boolean;
  prominentPrice?: boolean;
  hasOrder?: boolean;
  price?: string | null;
  credits?: string | null;
  buyDisabledReason?: string;
  favorited?: boolean;
  tryOn?: ReactNode;
  addToCartLabel?: string;
  onToggleFavorite?: () => void;
  onBuy?: () => void;
  onAddToCart?: () => void;
  onMakeOffer?: () => void;
  onViewListing?: (index: number) => void;
  onBack?: () => void;
};

export default function MkAssetDetailView({
  nft = undefined,
  listings = [],
  cartAnnounce = "",
  error = null,
  primaryMintOnly = false,
  prominentPrice = false,
  hasOrder = false,
  price = null,
  credits = null,
  buyDisabledReason = undefined,
  favorited = false,
  tryOn = undefined,
  addToCartLabel = undefined,
  onToggleFavorite = undefined,
  onBuy = undefined,
  onAddToCart = undefined,
  onMakeOffer = undefined,
  onViewListing = undefined,
  onBack = undefined,
}: MkAssetDetailViewProps) {
  const banner = (
    <>
      {error && (
        <div className="mkasset-route__deferred" role="alert">
          {error}
        </div>
      )}
      {primaryMintOnly && (
        <div className="mkasset-route__unlisted" role="note">
          {PRIMARY_MINT_NOTE}
        </div>
      )}
      {prominentPrice && hasOrder && (
        <div className="mkasset-route__prominent" data-variant="treatment">
          <div className="mkasset-route__pricebig">
            {credits != null && credits !== "0" ? (
              <>
                <span className="mkasset-route__pricemana"><Coin size={22} /></span>
                <span className="mkasset-route__priceval">{credits}</span>
                <span className="mkasset-route__pricelabel">{creditsNoun(credits, true)}</span>
              </>
            ) : price === "0" || credits === "0" ? (
              <span className="mkasset-route__priceval">Free</span>
            ) : price ? (
              <>
                <span className="mkasset-route__pricemana">&#x25C7;</span>
                <span className="mkasset-route__priceval">{price}</span>
                <span className="mkasset-route__pricelabel">MANA</span>
              </>
            ) : (
              <span className="mkasset-route__notforsale">Not for sale</span>
            )}
          </div>
          <button
            type="button"
            className="mkasset-route__buy"
            disabled={!!buyDisabledReason && !primaryMintOnly}
            onClick={
              buyDisabledReason
                ? primaryMintOnly
                  ? onMakeOffer
                  : undefined
                : price
                  ? onBuy
                  : onMakeOffer
            }
          >
            {price && !primaryMintOnly ? "Buy" : "Make an offer"}
          </button>
        </div>
      )}
    </>
  );

  return (
    <div className="mkasset-route">
      <div aria-live="polite" className="u-sr-only">
        {cartAnnounce}
      </div>
      <MkAssetPage
        nft={nft}
        listings={listings}
        emptyListings={listings.length === 0}
        favorited={favorited}
        banner={banner}
        tryOn={tryOn}
        buyDisabledReason={buyDisabledReason}
        onToggleFavorite={onToggleFavorite}
        onBuy={onBuy}
        onAddToCart={onAddToCart}
        addToCartLabel={addToCartLabel}
        onMakeOffer={onMakeOffer}
        onViewListing={onViewListing}
        onBack={onBack}
      />
    </div>
  );
}

export function MkAssetNotFound() {
  return (
    <main className="mkasset-route mkasset-route--missing" style={{ padding: 48, color: "#fff" }}>
      <h1>Asset not found</h1>
      <p style={{ opacity: 0.7 }}>
        This item is no longer available, or its data is missing from the
        catalog.
      </p>
      <a
        href="/shop"
        style={{
          display: "inline-block",
          marginTop: 16,
          padding: "10px 18px",
          borderRadius: 8,
          background: "#a855f7",
          color: "#fff",
          fontWeight: 600,
          textDecoration: "none",
        }}
      >
        Browse the shop
      </a>
    </main>
  );
}
