import type { CSSProperties } from "react";
import { useState } from "react";
import { MarketplaceChromeMaybe, type MarketplaceNavId } from "../frames/MarketplaceChrome";
import "./mkaccountcollectionssection.css";
import { Caret } from "../../atoms/icons";
import SearchField from "../../atoms/SearchField";

const ASSET_SECTIONS = [
  { id: "wearables", label: "Wearables" },
  { id: "emotes", label: "Emotes" },
  { id: "ens", label: "Names" },
  { id: "land", label: "Land" },
  { id: "collections", label: "Collections" },
];
const STORE_SECTIONS = [
  { id: "on_sale", label: "On Sale" },
  { id: "on_rent", label: "On Rent" },
  { id: "sales", label: "Sales" },
  { id: "bids", label: "Bids" },
  { id: "store_settings", label: "Settings" },
];

const COLLECTIONS_PER_PAGE = 12;

type SortOption = { value: string; text: string };

const SORT_OPTIONS: SortOption[] = [
  { value: "name", text: "Name" },
  { value: "newest", text: "Newest" },
  { value: "recently_reviewed", text: "Recently reviewed" },
  { value: "size", text: "Size" },
];

export type Collection = {
  contractAddress: string;
  name: string;
  size: number;
  isOnSale: boolean;
  tiles: string[];
};

const ListedBadge = () => (
  <span className="cl__badge" role="img" title="Listed for sale" aria-label="Listed for sale">
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M3 11.5V4a1 1 0 0 1 1-1h7.5L21 12.5 12.5 21 3 11.5Z" />
      <circle cx="7.5" cy="7.5" r="1.3" fill="currentColor" stroke="none" />
    </svg>
  </span>
);

function CollectionImage({ tiles }: { tiles: string[] }) {
  const row1 = tiles.slice(0, 2);
  const row2 = tiles.slice(2, 4);
  const rowH = row2.length ? "50%" : "100%";
  const Row = ({ items, full }: { items: string[]; full?: boolean }) => (
    <div className={"cl__imgrow" + (full ? " cl__imgrow--full" : "")} style={{ height: rowH }}>
      {items.map((rarity, i) => (
        <span
          key={i}
          className="cl__imgtile u-rar-bg"
          style={{ "--rb": `var(--rar-bg-${rarity})` } as CSSProperties}
          aria-hidden="true"
        />
      ))}
    </div>
  );
  if (tiles.length === 0) return <div className="cl__imgrow cl__imgrow--empty" />;
  return (
    <div className="cl__image">
      {row1.length > 0 ? <Row items={row1} full={tiles.length === 2} /> : null}
      {row2.length > 0 ? <Row items={row2} /> : null}
    </div>
  );
}

type MkAccountCollectionsSectionProps = {
  collections?: Collection[];
  count?: number;
  page?: number;
  sortBy?: string;
  isLoading?: boolean;
  chrome?: boolean;
};

export default function MkAccountCollectionsSection({
  collections = [],
  count = collections.length,
  page = 1,
  sortBy = "newest",
  isLoading = false,
  chrome = true,
}: MkAccountCollectionsSectionProps) {
  const [tab, setTab] = useState<MarketplaceNavId>("my-assets");
  const [section, setSection] = useState("collections");
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState(sortBy);
  const [sortOpen, setSortOpen] = useState(false);
  const [activePage, setActivePage] = useState(page);

  const pages = Math.ceil(count / COLLECTIONS_PER_PAGE);
  const hasPagination = pages > 1;
  const sortLabel = SORT_OPTIONS.find((o) => o.value === sort)?.text ?? "Newest";

  return (
    <MarketplaceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
      <div className="cl">
        <aside className="cl__sidebar" aria-label="My assets">
          <nav className="cl__menu" aria-label="Assets">
            <div className="cl__menuhead">ASSETS</div>
            <ul className="cl__menulist">
              {ASSET_SECTIONS.map((s) => (
                <li key={s.id}>
                  <button
                    type="button"
                    className={"cl__menuitem" + (s.id === section ? " is-active" : "")}
                    aria-current={s.id === section ? "true" : undefined}
                    onClick={() => setSection(s.id)}
                  >
                    {s.label}
                  </button>
                </li>
              ))}
            </ul>
          </nav>

          <nav className="cl__menu" aria-label="Store">
            <div className="cl__menuhead">STORE</div>
            <ul className="cl__menulist">
              {STORE_SECTIONS.map((s) => (
                <li key={s.id}>
                  <button
                    type="button"
                    className={"cl__menuitem" + (s.id === section ? " is-active" : "")}
                    aria-current={s.id === section ? "true" : undefined}
                    onClick={() => setSection(s.id)}
                  >
                    {s.label}
                  </button>
                </li>
              ))}
            </ul>
          </nav>
        </aside>

        <section className="cl__main" aria-label="Collections">
          <div className="cl__filters">
            <div className="cl__search">
              <SearchField
                value={search}
                onChange={setSearch}
                placeholder={`Search ${isLoading ? 0 : count} collections...`}
              />
            </div>
            <div className="cl__sort">
              <button
                type="button"
                className="cl__sortbtn"
                aria-haspopup="listbox"
                aria-expanded={sortOpen}
                onClick={() => setSortOpen((o) => !o)}
              >
                <span>{sortLabel}</span>
                <Caret size={12} />
              </button>
              {sortOpen && (
                <ul className="cl__sortmenu" role="listbox">
                  {SORT_OPTIONS.map((o) => (
                    <li key={o.value} role="option" aria-selected={o.value === sort}>
                      <button
                        type="button"
                        className={"cl__sortopt" + (o.value === sort ? " is-active" : "")}
                        onClick={() => {
                          setSort(o.value);
                          setSortOpen(false);
                        }}
                      >
                        {o.text}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>

          {isLoading ? (
            <div className="cl__loaderbox">
              <span className="cl__loader" role="status" aria-label="Loading" />
            </div>
          ) : collections.length === 0 ? (
            <div className="cl__empty">
              <div className="cl__emptyicon" aria-hidden="true">
                <svg viewBox="0 0 24 24" width="34" height="34" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
                  <rect x="3" y="3" width="8" height="8" rx="1.5" />
                  <rect x="13" y="3" width="8" height="8" rx="1.5" />
                  <rect x="3" y="13" width="8" height="8" rx="1.5" />
                  <rect x="13" y="13" width="8" height="8" rx="1.5" />
                </svg>
              </div>
              <p className="cl__emptytitle">No collections yet</p>
              <p className="cl__emptysub">Collections you create will appear here.</p>
              <a
                className="cl__emptycta"
                href="https://builder.decentraland.org/collections"
                target="_blank"
                rel="noopener noreferrer"
              >
                Create a collection
              </a>
            </div>
          ) : (
            <>
              <div className="cl__cards">
                {collections.map((c) => (
                  <a key={c.contractAddress} className="cl__card" href={`/marketplace/collection?contract=${c.contractAddress}`}>
                    <div className="cl__cardcontent">
                      <div className="cl__details">
                        <div className="cl__detailsleft">
                          <CollectionImage tiles={c.tiles} />
                        </div>
                        <div className="cl__detailsright">
                          <div className="cl__name u-truncate">{c.name}</div>
                          <div className="cl__count">{c.size} items</div>
                        </div>
                      </div>
                      {c.isOnSale && <ListedBadge />}
                    </div>
                  </a>
                ))}
              </div>

              {hasPagination && (
                <div className="cl__pagination">
                  <button
                    type="button"
                    className="cl__pagebtn cl__pagebtn--prev"
                    disabled={activePage <= 1}
                    onClick={() => setActivePage((p) => Math.max(1, p - 1))}
                    aria-label="Previous page"
                  >
                    ‹
                  </button>
                  {Array.from({ length: pages }, (_, i) => i + 1).map((p) => (
                    <button
                      key={p}
                      type="button"
                      className={"cl__pagebtn" + (p === activePage ? " is-active" : "")}
                      aria-current={p === activePage ? "page" : undefined}
                      onClick={() => setActivePage(p)}
                    >
                      {p}
                    </button>
                  ))}
                  <button
                    type="button"
                    className="cl__pagebtn cl__pagebtn--next"
                    disabled={activePage >= pages}
                    onClick={() => setActivePage((p) => Math.min(pages, p + 1))}
                    aria-label="Next page"
                  >
                    ›
                  </button>
                </div>
              )}
            </>
          )}
        </section>
      </div>
    </MarketplaceChromeMaybe>
  );
}
