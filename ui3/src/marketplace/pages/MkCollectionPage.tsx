import type { CSSProperties } from "react";
import { useState } from "react";
import { MarketplaceChromeMaybe, type MarketplaceNavId } from "../frames/MarketplaceChrome";
import ManaMark from "../../atoms/ManaMark";
import { Coin } from "../../atoms/icons";
import { creditsSrLabel } from "../credits-unit";
import "./mkcollectionpage.css";

type StyleVars = CSSProperties & Record<`--${string}`, string | number>;
const styleVars = (v: StyleVars): CSSProperties => v;

type Rarity =
  | "unique"
  | "mythic"
  | "exotic"
  | "legendary"
  | "epic"
  | "rare"
  | "uncommon"
  | "common";

const TABS = [
  { id: "wearables", label: "Wearables", category: "wearable" },
  { id: "emotes", label: "Emotes", category: "emote" },
];

type RarityInfo = { label: string; token: string; max: number };

const RARITY: Record<Rarity, RarityInfo> = {
  unique: { label: "Unique", token: "--rar-unique", max: 1 },
  mythic: { label: "Mythic", token: "--rar-mythic", max: 10 },
  exotic: { label: "Exotic", token: "--rar-exotic", max: 50 },
  legendary: { label: "Legendary", token: "--rar-legendary", max: 100 },
  epic: { label: "Epic", token: "--rar-epic", max: 1000 },
  rare: { label: "Rare", token: "--rar-rare", max: 5000 },
  uncommon: { label: "Uncommon", token: "--rar-uncommon", max: 10000 },
  common: { label: "Common", token: "--rar-common", max: 100000 },
};

const CATEGORY_LABEL: Record<string, string> = {
  hat: "Hat",
  upper_body: "Upper Body",
  lower_body: "Lower Body",
  feet: "Feet",
  eyewear: "Eyewear",
  hair: "Hair",
  head: "Head",
  helmet: "Helmet",
  dance: "Dance",
  fun: "Fun",
  greetings: "Greetings",
  poses: "Poses",
};

type Collection = {
  name: string;
  isOnSale?: boolean;
};

type CollectionItem = {
  id: string;
  name: string;
  category: "wearable" | "emote";
  sub: string;
  rarity: Rarity;
  available: number;
  price: string;
  credits?: string | null;
  image?: string;
};

function AssetCell({ item }: { item: CollectionItem }) {
  const r = RARITY[item.rarity];
  return (
    <a className="cp__assetcell" href={`/marketplace/${item.id}`}>
      <span
        className="cp__thumb u-rar-bg"
        style={styleVars({ "--rb": `var(--rar-bg-${item.rarity})` })}
        aria-hidden="true"
      >
        {item.image ? (
          <img className="cp__thumbimg" src={item.image} alt="" loading="lazy" />
        ) : (
          <span className="cp__glyph" style={{ background: `var(${r.token})` }} />
        )}
      </span>
      <span className="cp__assetname">{item.name}</span>
    </a>
  );
}

function OwnerEllipsis() {
  return (
    <div className="cp__ellipsis" aria-label="Item actions" role="button" tabIndex={0}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
        <circle cx="5" cy="12" r="2" />
        <circle cx="12" cy="12" r="2" />
        <circle cx="19" cy="12" r="2" />
      </svg>
    </div>
  );
}

type MkCollectionPageProps = {
  collection?: Collection;
  items?: CollectionItem[];
  isOwner?: boolean;
  state?: "ready" | "loading" | "empty";
  chrome?: boolean;
};

export default function MkCollectionPage({
  collection,
  items = [],
  isOwner = false,
  state = "ready",
  chrome = true,
}: MkCollectionPageProps) {
  const [navTab, setNavTab] = useState<MarketplaceNavId>("collectibles");

  const hasWearables = items.some((i) => i.category === "wearable");
  const hasEmotes = items.some((i) => i.category === "emote");
  const showTabs = hasWearables && hasEmotes;

  const [tab, setTab] = useState("wearables");

  const isLoading = state === "loading";
  const noCollection = state === "empty" || !collection;

  const filtered = showTabs
    ? items.filter((i) => (tab === "wearables" ? i.category === "wearable" : i.category === "emote"))
    : items;

  const headers = ["Item", "Category", "Rarity", "Stock", "Price"];

  return (
    <MarketplaceChromeMaybe chrome={chrome} active={navTab} onTab={setNavTab}>
      <div className="cp">
        <div className="cp__page">
          {isLoading ? (
            <div className="cp__loaderwrap">
              <span className="cp__loader" role="status" aria-label="Loading" />
            </div>
          ) : noCollection ? (
            <div className="cp__nocollection">No Collection</div>
          ) : (
            <>
              <section className="cp__section">
                <div className="cp__headerrow">
                  <div className="cp__headerleft">
                    <button type="button" className="cp__back" aria-label="Back">
                      <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                        <path d="M15 18l-6-6 6-6" />
                      </svg>
                    </button>
                    <h1 className="cp__title">{collection.name}</h1>
                    {collection.isOnSale ? (
                      <span className="cp__badge">
                        <svg viewBox="0 0 24 24" width="13" height="13" fill="currentColor" aria-hidden="true">
                          <path d="M20 12l-8 8-9-9V3h8l9 9zM7 7a1.5 1.5 0 1 0 0 .01z" />
                        </svg>
                        <span className="cp__badgetext">On Sale</span>
                      </span>
                    ) : null}
                  </div>

                  {isOwner ? (
                    <div className="cp__owneractions">
                      <button type="button" className="cp__obtn">
                        Edit in builder
                      </button>
                      <button type="button" className="cp__obtn">
                        {collection.isOnSale ? "Unlist from market" : "List on market"}
                      </button>
                    </div>
                  ) : null}
                </div>
              </section>

              <section className="cp__section">
                <div className="cp__tablecontainer">
                  {showTabs ? (
                    <div className="cp__tabsrow" role="tablist">
                      {TABS.map((tb) => (
                        <button
                          key={tb.id}
                          type="button"
                          role="tab"
                          aria-selected={tab === tb.id}
                          className={"cp__tab" + (tab === tb.id ? " is-active" : "")}
                          onClick={() => setTab(tb.id)}
                        >
                          {tb.label}
                        </button>
                      ))}
                    </div>
                  ) : null}

                  <div className={"cp__table" + (showTabs ? "" : " cp__table--noheaders")}>
                    {filtered.length === 0 ? (
                      <div className="cp__empty">No Collection</div>
                    ) : (
                      <table className="cp__tbl">
                        <thead>
                          <tr>
                            {headers.map((h) => (
                              <th key={h}>
                                <span>{h}</span>
                              </th>
                            ))}
                            {isOwner ? (
                              <th>
                                <span className="u-sr-only">Actions</span>
                              </th>
                            ) : null}
                          </tr>
                        </thead>
                        <tbody>
                          {filtered.map((item) => {
                            const r = RARITY[item.rarity];
                            return (
                              <tr key={item.id}>
                                <td className="cp__td-item">
                                  <AssetCell item={item} />
                                </td>
                                <td>{CATEGORY_LABEL[item.sub] || item.sub}</td>
                                <td>{r.label}</td>
                                <td>
                                  {item.available.toLocaleString()}/{r.max.toLocaleString()}
                                </td>
                                <td>
                                  {item.credits != null ? (
                                    <span className="cp__mana">
                                      <Coin size={13} className="cp__manamark" />
                                      {item.credits}
                                      <span className="u-sr-only">{creditsSrLabel(item.credits)}</span>
                                    </span>
                                  ) : item.price === "\u{2014}" ? (
                                    "-"
                                  ) : (
                                    <span className="cp__mana">
                                      <ManaMark size={13} className="cp__manamark" />
                                      {item.price}
                                      <span className="u-sr-only"> MANA</span>
                                    </span>
                                  )}
                                </td>
                                {isOwner ? (
                                  <td className="cp__td-actions">
                                    <OwnerEllipsis />
                                  </td>
                                ) : null}
                              </tr>
                            );
                          })}
                        </tbody>
                      </table>
                    )}
                  </div>
                </div>
              </section>
            </>
          )}
        </div>
      </div>
    </MarketplaceChromeMaybe>
  );
}
