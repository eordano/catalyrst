import type { ComponentProps, MouseEvent } from "react";

import AssetCard from "../components/AssetCard";
import EmptyState from "../../components/EmptyState";
import "./browselayout.css";
import "./listslayout.css";

type AnchorClick = (e: MouseEvent<HTMLAnchorElement>) => void;

export type MkListPreview = {
  image?: string;
  rarity: string;
};

export type MkListCard = {
  id: string;
  name: string;
  description?: string | null;
  isPrivate?: boolean;
  pickedByUser?: boolean;
  itemsCount: number;
  previews: MkListPreview[];
};

export type MkListItemCard = {
  id: string;
  name: string;
  collection?: string;
  price?: string | null;
  credits?: string | null;
  rarity: string;
  network: "ethereum" | "polygon";
  image?: string;
};

export type MkOpenList = {
  id: string;
  name: string;
  description?: string | null;
  isPrivate?: boolean;
  pickedByUser?: boolean;
  itemsCount: number;
  items: MkListItemCard[];
};

export function MkListsSignedOut({
  connecting = false,
  onSignIn = undefined,
}: {
  connecting?: boolean;
  onSignIn?: () => void;
}) {
  return (
    <div className="mklists">
      <div className="mklists__head">
        <h1 className="mklists__title">My Lists</h1>
      </div>
      {connecting ? (
        <p className="mklists__sub" role="status">
          Loading your lists…
        </p>
      ) : (
        <EmptyState
          role="status"
          variant="inline"
          title="Sign in to see your lists."
          subtitle="Your saved item lists are tied to your account."
          actions={[{ label: "Sign in", onClick: onSignIn }]}
        />
      )}
    </div>
  );
}

export function MkListsOverview({
  lists = [],
  onOpenList = undefined,
  onFavoritesClick = undefined,
}: {
  lists?: MkListCard[];
  onOpenList?: (id: string, e: MouseEvent<HTMLAnchorElement>) => void;
  onFavoritesClick?: AnchorClick;
}) {
  return (
    <div className="mklists">
      <div className="mklists__head">
        <h1 className="mklists__title">My Lists</h1>
      </div>
      <p className="mklists__sub">
        Saved-item collections. Open a list to browse the items inside.
      </p>

      {lists.length > 0 ? (
        <div className="mklists__grid">
          {lists.map((list) => (
            <a
              key={list.id}
              href={`/marketplace/lists?listId=${encodeURIComponent(list.id)}`}
              className="mklist-card"
              onClick={(e) => onOpenList?.(list.id, e)}
              aria-label={list.name}
            >
              <div className="mklist-card__mosaic">
                {[0, 1, 2, 3].map((i) => {
                  const p = list.previews[i];
                  return p ? (
                    <span
                      key={i}
                      className="mklist-card__cell"
                      style={{ ["--rar" as string]: `var(--rar-${p.rarity})` }}
                    >
                      {p.image ? <img src={p.image} alt="" /> : null}
                    </span>
                  ) : (
                    <span key={i} className="mklist-card__cell mklist-card__cell--empty" />
                  );
                })}
              </div>
              <div className="mklist-card__body">
                <div className="mklist-card__row">
                  <h2 className="mklist-card__name">{list.name}</h2>
                  {list.pickedByUser ? (
                    <span className="mklist-card__badge mklist-card__badge--fav">
                      Favorites
                    </span>
                  ) : null}
                  {list.isPrivate ? (
                    <span className="mklist-card__badge">Private</span>
                  ) : null}
                </div>
                {list.description ? (
                  <p className="mklist-card__desc">{list.description}</p>
                ) : null}
                <span className="mklist-card__count">
                  {list.itemsCount} {list.itemsCount === 1 ? "item" : "items"}
                </span>
              </div>
            </a>
          ))}
        </div>
      ) : (
        <p className="mkbrowse__empty">
          You have no saved lists yet. Hearts you tap on marketplace items are
          saved on this device under{" "}
          <a
            href="/shop?tab=my-favorites"
            style={{ color: "inherit" }}
            onClick={onFavoritesClick}
          >
            My Favorites
          </a>{" "}
          in the Shop.
        </p>
      )}
    </div>
  );
}

export function MkListsOpenList({
  list,
  onItemClick = undefined,
  onBackClick = undefined,
}: {
  list: MkOpenList;
  onItemClick?: (id: string, e: MouseEvent<HTMLAnchorElement>) => void;
  onBackClick?: AnchorClick;
}) {
  return (
    <div className="mklists">
      <div className="mklists__head">
        <div>
          <h1 className="mklists__title">{list.name}</h1>
          <p className="mklists__sub">
            {list.itemsCount} {list.itemsCount === 1 ? "item" : "items"}
            {list.isPrivate ? " · Private" : ""}
            {list.pickedByUser ? " · Your favorites" : ""}
          </p>
        </div>
        <a href="/marketplace/lists" className="mklists__back" onClick={onBackClick}>
          ← All lists
        </a>
      </div>

      {list.description ? (
        <p className="mklists__sub">{list.description}</p>
      ) : null}

      {list.items.length > 0 ? (
        <div className="mkbrowse__grid">
          {list.items.map((card) => (
            <a
              key={card.id}
              href={`/marketplace/${encodeURIComponent(card.id)}`}
              onClick={(e) => onItemClick?.(card.id, e)}
              aria-label={card.name}
              style={{ display: "block", textDecoration: "none", color: "inherit" }}
            >
              <AssetCard
                {...(({
                  name: card.name,
                  collection: card.collection,
                  price: card.credits ?? card.price ?? undefined,
                  unit: card.credits != null ? "credits" : "mana",
                  rarity: card.rarity,
                  network: card.network,
                  image: card.image,
                } satisfies Partial<ComponentProps<typeof AssetCard>>) as ComponentProps<typeof AssetCard>)}
              />
            </a>
          ))}
        </div>
      ) : (
        <EmptyState variant="inline" title="This list has no items yet." />
      )}
    </div>
  );
}
