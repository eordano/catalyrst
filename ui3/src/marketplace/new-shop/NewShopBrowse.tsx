import type { FormEvent, ReactNode } from "react";
import { useEffect, useState } from "react";
import CardGrid from "../../components/CardGrid";
import Dropdown from "../../components/Dropdown";
import SearchField from "../../atoms/SearchField";
import NewShopTabs, { NEW_SHOP_TABS, type NewShopTab } from "./NewShopTabs";
import NewShopFilterSidebar, { type FilterGroup } from "./NewShopFilterSidebar";
import NewShopAssetCard from "./NewShopAssetCard";
import type { ShopCard } from "./NewShopHome";
import "./newshopbrowse.css";

const DEFAULT_SORT = ["Recently listed", "Suggested", "Price: Low to High", "Price: High to Low", "Newest"];

type NewShopBrowseProps = {
  tabs?: readonly NewShopTab[];
  activeTab?: string;
  onTab?: (id: string) => void;
  groups?: FilterGroup[];
  cards?: ShopCard[];
  itemCount?: ReactNode;
  sortOptions?: string[];
  sortValue?: string;
  onSort?: (value: string) => void;
  onSale?: boolean;
  onToggleOnSale?: (on: boolean) => void;
  onOptionChange?: (groupId: string, optionId: string, checked: boolean) => void;
  searchValue?: string;
  searchPlaceholder?: string;
  onSearch?: (query: string) => void;
  filtersActive?: boolean;
  onClearFilters?: () => void;
  page?: number;
  totalPages?: number;
  onPageChange?: (page: number) => void;
  loading?: boolean;
  emptyLabel?: ReactNode;
  initialFavorites?: string[];
  favorites?: string[];
  onToggleFavorite?: (id: string, next: boolean) => void;
  onOpenAsset?: (id: string) => void;
  onBuyAsset?: (id: string) => void;
};

export default function NewShopBrowse({
  tabs = NEW_SHOP_TABS,
  activeTab = "all-assets",
  onTab,
  groups,
  cards = [],
  itemCount,
  sortOptions = DEFAULT_SORT,
  sortValue,
  onSort,
  onSale = true,
  onToggleOnSale,
  onOptionChange,
  searchValue = "",
  searchPlaceholder = "Search items, creators, collections…",
  onSearch,
  filtersActive = false,
  onClearFilters,
  page = 0,
  totalPages = 1,
  onPageChange,
  loading = false,
  emptyLabel = "No items match your filters.",
  initialFavorites = [],
  favorites,
  onToggleFavorite,
  onOpenAsset,
  onBuyAsset,
}: NewShopBrowseProps) {
  const [internalFavs, setInternalFavs] = useState<Set<string>>(() => new Set(initialFavorites));
  const favs = favorites ? new Set(favorites) : internalFavs;
  const [query, setQuery] = useState(searchValue);
  useEffect(() => setQuery(searchValue), [searchValue]);

  function submitSearch(e: FormEvent) {
    e.preventDefault();
    onSearch?.(query.trim());
  }

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
    <div className="mk nsbrowse">
      <NewShopTabs tabs={tabs} active={activeTab} onTab={onTab} />

      <div className="nsbrowse__body">
        <NewShopFilterSidebar
          groups={groups}
          itemCount={itemCount}
          onSale={onSale}
          onToggleOnSale={onToggleOnSale}
          onOptionChange={onOptionChange}
        />

        <div className="nsbrowse__main">
          <div className="nsbrowse__toolbar">
            {onSearch ? (
              <form className="nsbrowse__search" role="search" onSubmit={submitSearch}>
                <SearchField
                  value={query}
                  placeholder={searchPlaceholder}
                  onChange={setQuery}
                />
                <button type="submit" className="nsbrowse__search-go">Search</button>
              </form>
            ) : (
              <span className="nsbrowse__count">{itemCount}</span>
            )}
            <div className="nsbrowse__sort">
              <span className="nsbrowse__sort-label">Sort by</span>
              <Dropdown
                options={sortOptions}
                value={sortValue}
                defaultValue={sortOptions[0]}
                onChange={onSort}
                ariaLabel="Sort by"
              />
            </div>
          </div>

          <div
            className={"nsbrowse__results" + (loading ? " is-loading" : "")}
            aria-busy={loading}
          >
            {cards.length ? (
              <CardGrid min={200} gap={16}>
                {cards.map((c) => (
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
              </CardGrid>
            ) : (
              <div className="nsbrowse__empty">
                <span>{emptyLabel}</span>
                {filtersActive && onClearFilters ? (
                  <button type="button" className="nsbrowse__clear" onClick={onClearFilters}>
                    Clear filters
                  </button>
                ) : null}
              </div>
            )}
            {loading ? (
              <div className="nsbrowse__loading" aria-hidden="true">
                <span className="u-spinner" />
              </div>
            ) : null}
          </div>

          {cards.length && totalPages > 1 ? (
            <nav className="nsbrowse__pager" aria-label="Pagination">
              <button
                type="button"
                className="nsbrowse__page"
                aria-label="Previous page"
                disabled={page <= 0}
                onClick={() => onPageChange?.(page - 1)}
              >
                Previous
              </button>
              <span className="nsbrowse__pageinfo" aria-live="polite">
                Page {page + 1} of {totalPages}
              </span>
              <button
                type="button"
                className="nsbrowse__page"
                aria-label="Next page"
                disabled={page >= totalPages - 1}
                onClick={() => onPageChange?.(page + 1)}
              >
                Next
              </button>
            </nav>
          ) : null}
        </div>
      </div>
    </div>
  );
}
