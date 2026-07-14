import type { CSSProperties, ReactNode } from "react";
import ManaMark from "../../atoms/ManaMark";
import { Coin } from "../../atoms/icons";
import { creditsSrLabel } from "../credits-unit";
import "./newshopassetcard.css";

const RARITY_LABELS: Record<string, string> = {
  common: "Common",
  uncommon: "Uncommon",
  rare: "Rare",
  epic: "Epic",
  legendary: "Legendary",
  mythic: "Mythic",
  unique: "Unique",
  exotic: "Exotic",
};

const Heart = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path d="M12 20.5l-1.35-1.2C6.4 15.5 3.5 12.9 3.5 9.7 3.5 7.3 5.4 5.5 7.75 5.5c1.35 0 2.65.63 3.5 1.63A4.66 4.66 0 0 1 14.75 5.5C17.1 5.5 19 7.3 19 9.7c0 3.2-2.9 5.8-7.15 9.6z" />
  </svg>
);

type NewShopAssetCardProps = {
  name?: ReactNode;
  meta?: ReactNode;
  price?: ReactNode;
  unit?: "mana" | "credits";
  rarity?: string;
  network?: "polygon" | "ethereum";
  image?: string;
  favorited?: boolean;
  onToggleFavorite?: () => void;
  onOpen?: () => void;
  onBuy?: () => void;
  buyLabel?: ReactNode;
};

export default function NewShopAssetCard({
  name = "Asset Name",
  meta = "5d 12h ago",
  price,
  unit = "mana",
  rarity = "common",
  network = "polygon",
  image,
  favorited = false,
  onToggleFavorite,
  onOpen,
  onBuy,
  buyLabel = "Buy",
}: NewShopAssetCardProps) {
  return (
    <article className="nsac" style={{ "--rar": `var(--rar-${rarity})` } as CSSProperties}>
      <div className="nsac__media">
        {image ? (
          <img className="nsac__img" src={image} alt="" />
        ) : (
          <span className="nsac__placeholder" aria-hidden="true" />
        )}
        <button
          type="button"
          className={"nsac__fav" + (favorited ? " is-on" : "")}
          aria-label={favorited ? "Remove from favorites" : "Add to favorites"}
          aria-pressed={favorited}
          onClick={onToggleFavorite}
        >
          <Heart />
        </button>
        {price != null && onBuy ? (
          <button type="button" className="nsac__buy" onClick={onBuy}>
            {buyLabel}
          </button>
        ) : null}
      </div>

      <div className="nsac__body">
        <div className="nsac__row">
          <span className="nsac__name u-truncate">{name}</span>
          {price != null ? (
            <span className="nsac__price">
              {unit === "credits" ? (
                <Coin size={13} className="nsac__coinmark" />
              ) : (
                <ManaMark size={13} className={"nsac__manamark nsac__manamark--" + network} network={network} />
              )}
              {price}
              <span className="u-sr-only">{unit === "credits" ? creditsSrLabel(price) : " MANA"}</span>
            </span>
          ) : (
            <span className="nsac__notforsale">Not for sale</span>
          )}
        </div>
        <div className="nsac__meta">
          <span className="nsac__time u-truncate">{meta}</span>
          <span className="nsac__rarity">{RARITY_LABELS[rarity] || "Common"}</span>
        </div>
      </div>

      <button
        type="button"
        className="nsac__hit"
        aria-label={typeof name === "string" ? name : "Open asset"}
        onClick={onOpen}
      />
    </article>
  );
}
