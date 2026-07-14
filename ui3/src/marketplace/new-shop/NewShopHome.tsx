import type { ReactNode } from "react";
import { useState } from "react";
import NewShopTabs, { NEW_SHOP_TABS, type NewShopTab } from "./NewShopTabs";
import NewShopHeroBanner from "./NewShopHeroBanner";
import NewShopFeaturedRow from "./NewShopFeaturedRow";
import NewShopAssetCard from "./NewShopAssetCard";
import NewShopRankTable, { type RankRow } from "./NewShopRankTable";
import "./newshophome.css";

export type ShopCard = {
  id: string;
  name?: ReactNode;
  meta?: ReactNode;
  price?: ReactNode;
  unit?: "mana" | "credits";
  rarity?: string;
  network?: "polygon" | "ethereum";
  image?: string;
};

export type ShopBanner = {
  id: string;
  eyebrow?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  cta?: ReactNode;
  tone?: "purple" | "magenta" | "neon";
  art?: ReactNode;
};

export type FeaturedSection = {
  id: string;
  title: ReactNode;
  viewAllLabel?: ReactNode;
  cards: ShopCard[];
};

type NewShopHomeProps = {
  tabs?: readonly NewShopTab[];
  activeTab?: string;
  onTab?: (id: string) => void;
  banners?: ShopBanner[];
  featured?: FeaturedSection[];
  rankTitle?: ReactNode;
  rankRows?: RankRow[];
  initialFavorites?: string[];
  favorites?: string[];
  onToggleFavorite?: (id: string, next: boolean) => void;
  onOpenAsset?: (id: string) => void;
  onBuyAsset?: (id: string) => void;
  onViewAll?: (sectionId: string) => void;
  onBannerCta?: (bannerId: string) => void;
  onOpenRank?: (id: string) => void;
};

export default function NewShopHome({
  tabs = NEW_SHOP_TABS,
  activeTab = "overview",
  onTab,
  banners = [],
  featured = [],
  rankTitle,
  rankRows = [],
  initialFavorites = [],
  favorites,
  onToggleFavorite,
  onOpenAsset,
  onBuyAsset,
  onViewAll,
  onBannerCta,
  onOpenRank,
}: NewShopHomeProps) {
  const [internalFavs, setInternalFavs] = useState<Set<string>>(() => new Set(initialFavorites));
  const favs = favorites ? new Set(favorites) : internalFavs;

  function toggleFav(id: string) {
    const on = !favs.has(id);
    if (!favorites) {
      setInternalFavs((prev) => {
        const next = new Set(prev);
        if (on) next.add(id);
        else next.delete(id);
        return next;
      });
    }
    onToggleFavorite?.(id, on);
  }

  return (
    <div className="mk nshome">
      <NewShopTabs tabs={tabs} active={activeTab} onTab={onTab} />

      <div className="nshome__body">
        {banners.length ? (
          <div className="nshome__banners">
            {banners.map((b) => (
              <NewShopHeroBanner
                key={b.id}
                eyebrow={b.eyebrow}
                title={b.title}
                subtitle={b.subtitle}
                cta={b.cta}
                tone={b.tone}
                art={b.art}
                onCta={() => onBannerCta?.(b.id)}
              />
            ))}
          </div>
        ) : null}

        {featured.map((section) => (
          <NewShopFeaturedRow
            key={section.id}
            title={section.title}
            viewAllLabel={section.viewAllLabel}
            onViewAll={() => onViewAll?.(section.id)}
          >
            {section.cards.map((c) => (
              <NewShopAssetCard
                key={c.id}
                name={c.name}
                meta={c.meta}
                price={c.price}
                unit={c.unit}
                rarity={c.rarity}
                network={c.network}
                image={c.image}
                favorited={favs.has(c.id)}
                onToggleFavorite={() => toggleFav(c.id)}
                onOpen={() => onOpenAsset?.(c.id)}
                onBuy={() => onBuyAsset?.(c.id)}
              />
            ))}
          </NewShopFeaturedRow>
        ))}

        {rankRows.length ? (
          <NewShopRankTable title={rankTitle} rows={rankRows} onRow={onOpenRank} />
        ) : null}
      </div>
    </div>
  );
}
