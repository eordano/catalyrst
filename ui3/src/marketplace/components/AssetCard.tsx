import type { CSSProperties, ReactNode } from "react";
import ManaMark from "../../atoms/ManaMark";
import { Coin } from "../../atoms/icons";
import { creditsSrLabel } from "../credits-unit";
import "./assetcard.css";

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

type AssetCardProps = {
  name?: ReactNode;
  collection?: ReactNode;
  price?: ReactNode;
  unit?: "mana" | "credits";
  rarity?: string;
  network?: string;
  image?: string;
  figure?: ReactNode;
  tag?: ReactNode;
  metaRight?: ReactNode;
  onClick?: () => void;
};

export default function AssetCard({
  name = "Untitled",
  collection,
  price,
  unit = "mana",
  rarity = "common",
  network = "polygon",
  image,
  figure,
  tag,
  metaRight,
  onClick,
}: AssetCardProps) {
  const net = network === "ethereum" ? "ethereum" : "polygon";
  const Root = onClick ? "button" : "span";
  return (
    <Root
      {...(onClick ? { type: "button" as const, onClick } : {})}
      className="ac"
      style={{ "--rar": `var(--rar-${rarity})` } as CSSProperties}
    >
      <span className="ac__art ac__art--solid">
        {figure ? (
          figure
        ) : image ? (
          <img className="ac__img" src={image} alt="" />
        ) : (
          <span className="ac__placeholder" aria-hidden="true" />
        )}
        {tag ? <span className="ac__tag">{tag}</span> : null}
      </span>

      <span className="ac__body">
        <span className="ac__row">
          <span className="ac__name u-truncate">{name}</span>
          {price === "0" ? (
            <span className="ac__price">Free</span>
          ) : price != null ? (
            <span className="ac__price">
              {unit === "credits" ? (
                <Coin size={13} className="ac__coinmark" />
              ) : (
                <ManaMark size={13} className={"ac__manamark ac__manamark--" + net} network={net} />
              )}
              {price}
              <span className="u-sr-only">{unit === "credits" ? creditsSrLabel(price) : " MANA"}</span>
            </span>
          ) : (
            <span className="ac__notforsale">Not for sale</span>
          )}
        </span>
        <span className="ac__meta">
          {collection ? (
            <span className="ac__collection u-truncate">{collection}</span>
          ) : (
            <span className="ac__network">
              {net === "ethereum" ? "Ethereum" : "Polygon"}
            </span>
          )}
          {metaRight != null ? (
            <span className="ac__count">{metaRight}</span>
          ) : (
            <span className="ac__rarity">{RARITY_LABELS[rarity] || "Common"}</span>
          )}
        </span>
      </span>
    </Root>
  );
}
