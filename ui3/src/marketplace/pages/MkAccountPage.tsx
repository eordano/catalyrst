import type { CSSProperties } from "react";
import { useState } from "react";
import { MarketplaceChromeMaybe, type MarketplaceNavId } from "../frames/MarketplaceChrome";
import AssetCard from "../components/AssetCard";
import AssetTopbar from "../components/AssetTopbar";
import ManaMark from "../../atoms/ManaMark";
import FilterBox from "../../components/FilterBox";
import PriceRange from "../components/PriceRange";
import FilterRadios from "../../components/FilterRadios";
import EmptyState from "../../components/EmptyState";
import "./mkaccountpage.css";

type Option = { id: string; label: string };

type OwnedItem = {
  name: string;
  collection?: string;
  price?: string;
  rarity: string;
  network?: string;
};

type SaleListing = {
  name: string;
  collection: string;
  rarity: string;
  saleType: string;
  price: string;
  expiresIn: string;
};

type SaleRecord = {
  name: string;
  rarity: string;
  type: string;
  from: string;
  to: string;
  price: string;
  date: string;
};

type OpenKey = "status" | "rarity" | "price" | "sale";

const ASSET_SECTIONS = [
  { id: "wearables", label: "Wearables" },
  { id: "emotes", label: "Emotes" },
  { id: "ens", label: "Names" },
  { id: "land", label: "Land" },
  { id: "collections", label: "Collections" },
];

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

const STATUS_OPTIONS: Option[] = [
  { id: "all", label: "All" },
  { id: "sale", label: "On Sale" },
  { id: "notforsale", label: "Not for sale" },
];

const SORTS: Option[] = [
  { id: "newest", label: "Newest" },
  { id: "name_asc", label: "Name (A–Z)" },
  { id: "recently_listed", label: "Recently listed" },
  { id: "cheapest", label: "Cheapest" },
  { id: "most_expensive", label: "Most expensive" },
];

export default function MkAccountPage({
  owned = [],
  onSale = [],
  sales = [],
  initialSection = "wearables",
  chrome = true,
}: {
  owned?: OwnedItem[];
  onSale?: SaleListing[];
  sales?: SaleRecord[];
  initialSection?: string;
  chrome?: boolean;
}) {
  const [tab, setTab] = useState<MarketplaceNavId>("my-assets");
  const [section, setSection] = useState(initialSection);
  const [sort, setSort] = useState("newest");
  const [rarities, setRarities] = useState<string[]>([]);
  const [status, setStatus] = useState("all");
  const [onlyOnSale, setOnlyOnSale] = useState(false);

  const [open, setOpen] = useState<Record<OpenKey, boolean>>({ status: false, rarity: false, price: false, sale: true });
  const toggle = (k: OpenKey) => setOpen((o) => ({ ...o, [k]: !o[k] }));
  const toggleRarity = (id: string) =>
    setRarities((r) => (r.includes(id) ? r.filter((x) => x !== id) : [...r, id]));

  const isGridSection = section === "wearables" || section === "emotes" || section === "ens" || section === "land";
  const isListSection = section === "on_sale" || section === "on_rent" || section === "sales";
  const isCollections = section === "collections";
  const isSettings = section === "store_settings";

  const count = owned.length;

  return (
    <MarketplaceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
      <div className="ma">
        <aside className="ma__sidebar" aria-label="Asset filters">
          <nav className="ma__menu" aria-label="Asset type">
            <div className="ma__menuhead">ASSETS</div>
            <ul className="ma__menulist">
              {ASSET_SECTIONS.map((s) => (
                <li key={s.id}>
                  <button
                    type="button"
                    className={"ma__menuitem" + (s.id === section ? " is-active" : "")}
                    aria-current={s.id === section ? "true" : undefined}
                    onClick={() => setSection(s.id)}
                  >
                    {s.label}
                  </button>
                </li>
              ))}
            </ul>
          </nav>

          {isGridSection ? (
            <div className="ma__filters">
              <FilterBox title="On sale" open={open.sale} onToggle={() => toggle("sale")}>
                <label className="ma__toggle">
                  <span className="ma__togglelabel">Only on sale</span>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={onlyOnSale}
                    className={"ma__switch" + (onlyOnSale ? " is-on" : "")}
                    onClick={() => setOnlyOnSale((v) => !v)}
                  >
                    <span className="ma__switchknob" />
                  </button>
                </label>
              </FilterBox>

              <FilterBox title="Status" open={open.status} onToggle={() => toggle("status")}>
                <FilterRadios name="ma-status" value={status} onChange={setStatus} options={STATUS_OPTIONS} />
              </FilterBox>

              <FilterBox title="Rarity" open={open.rarity} onToggle={() => toggle("rarity")}>
                <div className="ma__chips">
                  {RARITIES.map((r) => (
                    <button
                      key={r.id}
                      type="button"
                      className={"ma__chip" + (rarities.includes(r.id) ? " is-active" : "")}
                      aria-pressed={rarities.includes(r.id)}
                      style={{ "--chip": `var(--rar-${r.id})` } as CSSProperties}
                      onClick={() => toggleRarity(r.id)}
                    >
                      <span className="ma__chipdot" />
                      {r.label}
                    </button>
                  ))}
                </div>
              </FilterBox>

              <FilterBox title="Price" open={open.price} onToggle={() => toggle("price")}>
                <PriceRange />
              </FilterBox>
            </div>
          ) : null}
        </aside>

        <section className="ma__main" aria-label="My assets">
          {isGridSection ? (
            <>
              <AssetTopbar
                layout="stacked"
                searchPlaceholder="Search"
                count={count}
                sort={sort}
                onSort={setSort}
                sortOptions={SORTS}
              />

              {count ? (
                <>
                  <div className="ma__grid">
                    {owned.map((item, i) => (
                      <div className="ma__cell" key={i}>
                        <AssetCard {...item} />
                      </div>
                    ))}
                  </div>
                  <div className="ma__loadmore">
                    <button type="button" className="ma__loadbtn">Load more</button>
                  </div>
                </>
              ) : (
                <EmptyState
                  icon={
                    <svg viewBox="0 0 24 24" width="34" height="34" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M3 7l9-4 9 4-9 4-9-4Z" />
                      <path d="M3 7v10l9 4 9-4V7" />
                      <path d="M12 11v10" />
                    </svg>
                  }
                  title="No assets yet"
                  subtitle="Items you own will show up here."
                />
              )}
            </>
          ) : null}

          {isListSection ? (
            <div className="ma__list">
              <div className="ma__listhead">
                {section === "sales" ? (
                  <>
                    <span className="ma__col ma__col--item">Item</span>
                    <span className="ma__col">Type</span>
                    <span className="ma__col">From</span>
                    <span className="ma__col">To</span>
                    <span className="ma__col ma__col--price">Price</span>
                    <span className="ma__col">Date</span>
                  </>
                ) : (
                  <>
                    <span className="ma__col ma__col--item">Item</span>
                    <span className="ma__col">Type</span>
                    <span className="ma__col">Sale type</span>
                    <span className="ma__col ma__col--price">Price</span>
                    <span className="ma__col">Expires</span>
                  </>
                )}
              </div>

              {section === "sales"
                ? sales.map((r, i) => (
                    <div className="ma__row" key={i}>
                      <span className="ma__col ma__col--item">
                        <span className="ma__thumb u-rar-bg" style={{ "--rb": `var(--rar-bg-${r.rarity})` } as CSSProperties} />
                        <span className="ma__itemname u-truncate">{r.name}</span>
                      </span>
                      <span className="ma__col">{r.type}</span>
                      <span className="ma__col ma__addr">{r.from}</span>
                      <span className="ma__col ma__addr">{r.to}</span>
                      <span className="ma__col ma__col--price">
                        <span className="ma__mana"><ManaMark size={12} className="ma__manamark" />{r.price}</span>
                      </span>
                      <span className="ma__col ma__muted">{r.date}</span>
                    </div>
                  ))
                : onSale.map((r, i) => (
                    <div className="ma__row" key={i}>
                      <span className="ma__col ma__col--item">
                        <span className="ma__thumb u-rar-bg" style={{ "--rb": `var(--rar-bg-${r.rarity})` } as CSSProperties} />
                        <span className="ma__itemcell">
                          <span className="ma__itemname u-truncate">{r.name}</span>
                          <span className="ma__itemsub u-truncate">{r.collection}</span>
                        </span>
                      </span>
                      <span className="ma__col">{section === "on_rent" ? "Rent" : "Wearable"}</span>
                      <span className="ma__col">{section === "on_rent" ? "Rental" : r.saleType}</span>
                      <span className="ma__col ma__col--price">
                        <span className="ma__mana"><ManaMark size={12} className="ma__manamark" />{r.price}</span>
                      </span>
                      <span className="ma__col ma__muted">{r.expiresIn}</span>
                    </div>
                  ))}
            </div>
          ) : null}

          {isCollections ? (
            <div className="ma__grid ma__grid--collections">
              {["NeonForge", "Skybound", "EdoStyle"].map((name, i) => (
                <div className="ma__collection" key={i}>
                  <span className="ma__collart u-rar-bg" style={{ "--rb": `var(--rar-bg-${["legendary", "mythic", "rare"][i]})` } as CSSProperties} />
                  <div className="ma__collbody">
                    <span className="ma__collname u-truncate">{name}</span>
                    <span className="ma__collmeta">{[12, 8, 5][i]} items · Published</span>
                  </div>
                </div>
              ))}
            </div>
          ) : null}

          {isSettings ? (
            <div className="ma__settingsnote">
              <p className="ma__notitle">Store Settings</p>
              <p className="ma__nosub">Edit your storefront cover, description and social links.</p>
            </div>
          ) : null}
        </section>
      </div>
    </MarketplaceChromeMaybe>
  );
}
