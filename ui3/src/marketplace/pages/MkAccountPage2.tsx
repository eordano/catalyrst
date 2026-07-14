import type { CSSProperties, ReactNode } from "react";
import { useState } from "react";
import { MarketplaceChromeMaybe, type MarketplaceNavId } from "../frames/MarketplaceChrome";
import AssetCard from "../components/AssetCard";
import AssetTopbar from "../components/AssetTopbar";
import { ChevronLeft, Caret } from "../../atoms/icons";
import PriceRange from "../components/PriceRange";
import "./mkaccountpage2.css";

type AssetType = "item" | "nft";
type Option = { id: string; label: string };

type ItemCard = {
  name: string;
  collection?: string;
  price?: string;
  rarity: string;
  network?: string;
};

type Account = {
  name: string;
  address: string;
  description?: string;
  cover?: string | null;
  links?: string[];
};

type OpenKey = "rarity" | "price";

const ALT_MENU: { id: AssetType; icon: string; main: string; detail: string }[] = [
  { id: "item", icon: "sparkles", main: "Originals", detail: "Created by the user" },
  { id: "nft", icon: "assets", main: "Collectibles", detail: "Collected by the user" },
];

const SECTIONS: Record<AssetType, Option[]> = {
  item: [
    { id: "wearables", label: "Wearables" },
    { id: "emotes", label: "Emotes" },
  ],
  nft: [
    { id: "all", label: "All Assets" },
    { id: "wearables", label: "Wearables" },
    { id: "land", label: "Land" },
    { id: "emotes", label: "Emotes" },
    { id: "ens", label: "Names" },
  ],
};

const RARITIES: Option[] = [
  { id: "common", label: "Common" },
  { id: "uncommon", label: "Uncommon" },
  { id: "rare", label: "Rare" },
  { id: "epic", label: "Epic" },
  { id: "legendary", label: "Legendary" },
  { id: "mythic", label: "Mythic" },
  { id: "unique", label: "Unique" },
  { id: "exotic", label: "Exotic" },
];

const SORTS: Option[] = [
  { id: "recently_listed", label: "Recently listed" },
  { id: "newest", label: "Newest" },
  { id: "cheapest", label: "Cheapest" },
  { id: "most_expensive", label: "Most expensive" },
  { id: "recently_sold", label: "Recently sold" },
];

function shortenAddress(addr: string) {
  return addr.length > 12 ? `${addr.slice(0, 6)}...${addr.slice(-4)}` : addr;
}

const LinkIcon = ({ type }: { type: string }) => {
  const glyph: Record<string, ReactNode> = {
    website: <path d="M12 3a9 9 0 100 18 9 9 0 000-18zm0 0c2.5 2.5 2.5 15.5 0 18m0-18c-2.5 2.5-2.5 15.5 0 18M3.5 9h17M3.5 15h17" />,
    facebook: <path d="M14 8.5h2.5V5.5H14c-2 0-3.5 1.5-3.5 3.5v2H8v3h2.5V21h3v-7H16l.5-3h-3V9c0-.4.2-.5.5-.5z" fill="currentColor" stroke="none" />,
    twitter: <path d="M21 6.5c-.7.3-1.4.5-2.2.6.8-.5 1.4-1.2 1.7-2.1-.7.5-1.6.8-2.5 1A3.6 3.6 0 0012 9.3c0 .3 0 .6.1.8A10 10 0 014.5 5.4a3.6 3.6 0 001.1 4.8c-.6 0-1.1-.2-1.6-.4 0 1.7 1.2 3.2 2.9 3.5-.5.1-1 .2-1.5.1.4 1.4 1.7 2.4 3.2 2.4A7.2 7.2 0 013 17.4 10 10 0 008.5 19c6.6 0 10.2-5.5 10.2-10.2v-.5c.7-.5 1.3-1.1 1.8-1.8z" fill="currentColor" stroke="none" />,
    discord: <path d="M18.5 6.5A14 14 0 0015 5.3l-.2.4a13 13 0 014 2 13.5 13.5 0 00-11.6 0 13 13 0 014-2L11 5.3A14 14 0 005.5 6.5C3.5 9.6 3 12.7 3.2 15.7a14 14 0 004.3 2.2l.9-1.5a9 9 0 01-1.4-.7l.3-.2a10 10 0 008.5 0l.3.2c-.4.3-.9.5-1.4.7l.9 1.5a14 14 0 004.3-2.2c.3-3.5-.5-6.6-2.6-9.2zM9.3 14c-.8 0-1.5-.8-1.5-1.7s.6-1.7 1.5-1.7 1.5.8 1.5 1.7-.7 1.7-1.5 1.7zm5.4 0c-.8 0-1.5-.8-1.5-1.7s.6-1.7 1.5-1.7 1.5.8 1.5 1.7-.7 1.7-1.5 1.7z" fill="currentColor" stroke="none" />,
  };
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {glyph[type]}
    </svg>
  );
};

function SidebarSection({ title, children }: { title: ReactNode; children?: ReactNode }) {
  return (
    <div className="ap2__menu">
      <div className="ap2__subhead">{title}</div>
      {children}
    </div>
  );
}

export default function MkAccountPage2({
  account,
  items = [],
  state = "default",
  chrome = true,
}: {
  account?: Account;
  items?: ItemCard[];
  state?: string;
  chrome?: boolean;
}) {
  const [tab, setTab] = useState<MarketplaceNavId>("my-assets");
  const [assetType, setAssetType] = useState<AssetType>("nft");
  const [section, setSection] = useState("all");
  const [sort, setSort] = useState("newest");
  const [rarities, setRarities] = useState<string[]>([]);
  const [open, setOpen] = useState<Record<OpenKey, boolean>>({ rarity: true, price: false });

  const toggle = (k: OpenKey) => setOpen((o) => ({ ...o, [k]: !o[k] }));
  const toggleRarity = (id: string) =>
    setRarities((r) => (r.includes(id) ? r.filter((x) => x !== id) : [...r, id]));

  const pickAssetType = (id: AssetType) => {
    setAssetType(id);
    setSection(id === "item" ? "wearables" : "all");
  };

  const sections = SECTIONS[assetType];
  const showFilters = assetType === "item" || ["wearables", "emotes"].includes(section);
  const count = items.length;
  const isLoading = state === "loading";
  const isError = state === "error";
  const isEmpty = state === "empty";

  const banner = (
    <header className="ap2__banner">
      {account?.cover ? <img className="ap2__cover" src={account.cover} alt="cover" /> : <div className="ap2__cover ap2__cover--blank" aria-hidden="true" />}
      <div className="ap2__cover-top">
        <button type="button" className="ap2__back" aria-label="Back">
          <ChevronLeft size={18} />
        </button>
        <div className="ap2__links">
          {(account?.links || []).map((type) => (
            <button key={type} type="button" className="ap2__link" aria-label={type}>
              <LinkIcon type={type} />
            </button>
          ))}
        </div>
      </div>

      <div className="ap2__profile">
        <span className="ap2__avatar u-avatar" style={{ "--sz": "96px", "--hue": 312 } as CSSProperties} aria-hidden="true" />
        <div className="ap2__name">
          <a href={`/profile/${account?.address ?? ""}`} className="ap2__namelink">{account?.name ?? ""}</a>
        </div>
        <div className="ap2__address">
          <span className="ap2__hash">{shortenAddress(account?.address ?? "")}</span>
          <button type="button" className="ap2__copy" aria-label="Copy address">
            <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <rect x="9" y="9" width="11" height="11" rx="2" />
              <path d="M5 15V5a2 2 0 012-2h10" />
            </svg>
          </button>
        </div>
        {account?.description ? <p className="ap2__desc">{account.description}</p> : null}
      </div>
    </header>
  );

  const sidebar = (
    <aside className="ap2__sidebar" aria-label="Account sections">
      <div className="ap2__altmenu">
        {ALT_MENU.map((m) => (
          <button
            key={m.id}
            type="button"
            className={"ap2__alt" + (assetType === m.id ? " is-selected" : "")}
            aria-pressed={assetType === m.id}
            onClick={() => pickAssetType(m.id)}
          >
            <span className={"ap2__alticon ap2__alticon--" + m.icon} aria-hidden="true" />
            <span className="ap2__alttext">
              <span className="ap2__altmain">{m.main}</span>
              <span className="ap2__altdetail">{m.detail}</span>
            </span>
          </button>
        ))}
      </div>

      <SidebarSection title={assetType === "item" ? "CATEGORIES" : "ASSETS"}>
        <ul className="ap2__seclist">
          {sections.map((s) => (
            <li key={s.id}>
              <button
                type="button"
                className={"ap2__secitem" + (s.id === section ? " is-active" : "")}
                aria-current={s.id === section ? "true" : undefined}
                onClick={() => setSection(s.id)}
              >
                {s.label}
              </button>
            </li>
          ))}
        </ul>
      </SidebarSection>

      <SidebarSection title="Assets on rent">
        <ul className="ap2__seclist">
          <li>
            <button type="button" className="ap2__secitem">Land</button>
          </li>
        </ul>
      </SidebarSection>

      {showFilters ? (
        <div className="ap2__filters">
          <div className="ap2__fsection">
            <button type="button" className="ap2__fhead" aria-expanded={open.rarity} onClick={() => toggle("rarity")}>
              <span className="ap2__ftitle">Rarity</span>
              <Caret className="ap2__caret" open={open.rarity} />
            </button>
            {open.rarity ? (
              <div className="ap2__fbody">
                <div className="ap2__chips">
                  {RARITIES.map((r) => (
                    <button
                      key={r.id}
                      type="button"
                      className={"ap2__chip" + (rarities.includes(r.id) ? " is-active" : "")}
                      aria-pressed={rarities.includes(r.id)}
                      style={{ "--chip": `var(--rar-${r.id})` } as CSSProperties}
                      onClick={() => toggleRarity(r.id)}
                    >
                      <span className="ap2__chipdot" />
                      {r.label}
                    </button>
                  ))}
                </div>
              </div>
            ) : null}
          </div>
          <div className="ap2__fsection">
            <button type="button" className="ap2__fhead" aria-expanded={open.price} onClick={() => toggle("price")}>
              <span className="ap2__ftitle">Price</span>
              <Caret className="ap2__caret" open={open.price} />
            </button>
            {open.price ? (
              <div className="ap2__fbody">
                <PriceRange />
              </div>
            ) : null}
          </div>
        </div>
      ) : null}
    </aside>
  );

  const main = (
    <section className="ap2__main" aria-label="Assets">
      <AssetTopbar
        layout="stacked"
        searchPlaceholder="Search assets"
        showCount={!isLoading}
        count={count}
        sort={sort}
        onSort={setSort}
        sortOptions={SORTS}
      />

      {isLoading ? (
        <div className="ap2__loader" role="status" aria-live="polite">
          <span className="ap2__spinner" aria-hidden="true" />
          <span className="ap2__loadertext">Loading assets&#x2026;</span>
        </div>
      ) : isError ? (
        <div className="ap2__state">
          <p className="ap2__statetitle">That's not a valid address</p>
          <p className="ap2__statesub">Check the URL and try again.</p>
        </div>
      ) : isEmpty || count === 0 ? (
        <div className="ap2__state">
          <div className="ap2__noicon" aria-hidden="true">
            <svg viewBox="0 0 24 24" width="34" height="34" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="11" cy="11" r="7" />
              <path d="M21 21l-4.3-4.3" />
            </svg>
          </div>
          <p className="ap2__statetitle">No assets here yet</p>
          <p className="ap2__statesub">Try another section or adjust your filters.</p>
        </div>
      ) : (
        <>
          <div className="ap2__grid ap2__grid--grid">
            {items.map((item, i) => (
              <div className="ap2__cell" key={i}>
                <AssetCard {...item} />
              </div>
            ))}
          </div>
          <div className="ap2__loadmore">
            <button type="button" className="ap2__loadbtn">Load more</button>
          </div>
        </>
      )}
    </section>
  );

  return (
    <MarketplaceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
      <div className="ap2">
        {banner}
        <div className="ap2__browse">
          {sidebar}
          {main}
        </div>
      </div>
    </MarketplaceChromeMaybe>
  );
}
