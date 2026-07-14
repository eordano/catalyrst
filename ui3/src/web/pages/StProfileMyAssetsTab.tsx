import type { ReactNode } from "react";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import { suffixLabel, type LabelSuffixProps } from "../../components/labelSuffix";
import "./stprofilemyassetstab.css";

export type AssetItem = {
  id: string;
  name: string;
  rarity: string;
  price: string | null;
  network: string;
  category?: string | null;
  bodyShape: string;
  isSmart: boolean;
};

export type NameItem = { id: string; stem: string };

export type Profile = {
  address: string;
  name?: string | null;
  hasClaimedName?: boolean;
  nameColor?: string;
};

type TabItem = { id: string; label: string };

const RARITY_GRAD: Record<string, string> = {
  common: "radial-gradient(circle, #D2F9F9 0%, #73D3D3 100%)",
  uncommon: "radial-gradient(circle, #F9E4DF 0%, #FF8362 100%)",
  rare: "radial-gradient(circle, #C1F2D6 0%, #34CE76 100%)",
  epic: "radial-gradient(circle, #C0D3EF 0%, #438FFF 100%)",
  legendary: "radial-gradient(circle, #E1C1FF 0%, #A14BF3 100%)",
  exotic: "radial-gradient(circle, #D1E989 0%, #9BD141 100%)",
  mythic: "radial-gradient(circle, #FDC4F7 0%, #FF4BED 100%)",
  unique: "radial-gradient(circle, #F3E5CF 0%, #FEA217 100%)",
};

const CATEGORY_FILTERS = [
  { value: "wearable", label: "Wearables", icon: "checkroom" },
  { value: "emote", label: "Emotes", icon: "emote" },
  { value: "ens", label: "Names", icon: "at" },
  { value: "parcel", label: "Lands", icon: "map" },
  { value: "estate", label: "Estates", icon: "landscape" },
];

const TABS: TabItem[] = [
  { id: "overview", label: "Overview" },
  { id: "assets", label: "My Assets" },
  { id: "communities", label: "My Communities" },
  { id: "places", label: "My Places" },
  { id: "photos", label: "My Photos" },
  { id: "referral-rewards", label: "Referral Rewards" },
];

const EMPTY_PROFILE: Profile = { address: "" };

const ICONS: Record<string, ReactNode> = {
  checkroom: (
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
      <path d="M21.6 18.2 13 13.6V11a3 3 0 1 0-4-2.8" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M11 13.6 2.4 18.2a1 1 0 0 0 .5 1.8h18.2a1 1 0 0 0 .5-1.8" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  emote: (
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
      <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="1.6" />
      <circle cx="9" cy="10" r="1.1" fill="currentColor" />
      <circle cx="15" cy="10" r="1.1" fill="currentColor" />
      <path d="M8 14.5a5 5 0 0 0 8 0" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  ),
  at: (
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
      <circle cx="12" cy="12" r="3.4" fill="none" stroke="currentColor" strokeWidth="1.6" />
      <path d="M15.4 12v1.2a2.3 2.3 0 0 0 4.6 0V12a8 8 0 1 0-3.2 6.4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  ),
  map: (
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
      <path d="m9 4 6 2 6-2v14l-6 2-6-2-6 2V6l6-2Zm0 0v14m6-12v14" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
    </svg>
  ),
  landscape: (
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
      <path d="M3 19h18L14 8l-3 5-2-3-6 9Z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
    </svg>
  ),
};

const BODY_GLYPH: Record<string, { label: string; path: string }> = {
  male: { label: "For male", path: "M14 9a4 4 0 1 1-4-4M14 9l3-3m-2.4 0H17v2.4" },
  female: { label: "For female", path: "M12 4a4 4 0 0 1 0 8m0 0v6m-2.5-3h5" },
  unisex: { label: "Unisex", path: "M9 11a3 3 0 1 1 0-6 3 3 0 0 1 0 6Zm6 8a3 3 0 1 1 0-6 3 3 0 0 1 0 6Z" },
};

const ManaMark = () => (
  <svg className="stam__manamark" viewBox="0 0 24 24" width="13" height="13" aria-hidden="true">
    <path d="M12 2 3 12l9 10 9-10L12 2Z" fill="currentColor" opacity="0.92" />
  </svg>
);

const SmartGlyph = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <path d="M13 2 4 13h6l-1 9 9-11h-6l1-9Z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
  </svg>
);

const CategoryGlyph = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <path d="M4 7h16M4 12h16M4 17h10" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);

function AssetCard({ item }: { item: AssetItem }) {
  const body = BODY_GLYPH[item.bodyShape];
  return (
    <article className="stam__card">
      <div className="stam__thumb" style={{ background: RARITY_GRAD[item.rarity] || RARITY_GRAD.common }}>
        <span className="stam__thumbph" aria-hidden="true" />
        <div className="stam__badges">
          <span className="stam__badge" role="img" title="Category" aria-label="Category">
            <CategoryGlyph />
          </span>
          {body ? (
            <span className="stam__badge" role="img" title={body.label} aria-label={body.label}>
              <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
                <path d={body.path} fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </span>
          ) : null}
          {item.isSmart ? (
            <span className="stam__badge" role="img" title="Smart wearable" aria-label="Smart wearable">
              <SmartGlyph />
            </span>
          ) : null}
        </div>
      </div>
      <div className="stam__cardbody">
        <div className="stam__cardrow">
          <span className="stam__cardname u-truncate">{item.name}</span>
          {item.price != null ? (
            <span className="stam__price">
              <ManaMark />
              {item.price}
            </span>
          ) : (
            <span className="stam__notforsale">Not for sale</span>
          )}
        </div>
        <div className="stam__cardmeta">
          <span className="stam__rarity" style={{ color: `var(--rar-${item.rarity})` }}>
            {item.rarity}
          </span>
          <span className="stam__network">{item.network === "ETHEREUM" ? "Ethereum" : "Polygon"}</span>
        </div>
        <span className="stam__viewbtn" role="button" tabIndex={0}>
          View
        </span>
      </div>
    </article>
  );
}

type StProfileMyAssetsTabProps = LabelSuffixProps & {
  chrome?: boolean;
  profile?: Profile;
  tabs?: TabItem[];
  activeTab?: string;
  category?: string;
  wearables?: AssetItem[];
  names?: NameItem[];
  loading?: boolean;
  empty?: boolean;
};

export default function StProfileMyAssetsTab({
  labelSuffix,
  chrome = true,
  profile = EMPTY_PROFILE,
  tabs = TABS,
  activeTab = "assets",
  category = "wearable",
  wearables = [],
  names = [],
  loading = false,
  empty = false,
}: StProfileMyAssetsTabProps) {
  const initial = (profile.name || profile.address).charAt(0).toUpperCase();
  const isNames = category === "ens";
  const items = isNames ? names : wearables;
  const total = empty ? 0 : items.length;
  const showEmpty = empty || (!loading && total === 0);

  return (
    <SitesChromeMaybe chrome={chrome} active="legal" overlayNav>
      <div className="stam">
        <div className="stam__content">
          <div className="stam__card">
            <div className="stam__header">
              <div className="stam__identity">
                <span className="stam__avatar" style={{ background: profile.nameColor }} aria-hidden="true">
                  <span className="stam__avatarinitial">{initial}</span>
                </span>
                <div className="stam__nameblock">
                  <div className="stam__namerow">
                    <span className="stam__name" style={{ color: profile.nameColor }}>
                      {profile.name}
                    </span>
                    {profile.name ? (
                      <span className="stam__verified" style={{ background: profile.nameColor }} title="Verified">
                        <svg viewBox="0 0 24 24" width="11" height="11" aria-hidden="true">
                          <path d="m6 12 4 4 8-9" fill="none" stroke="#0f0e11" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
                        </svg>
                      </span>
                    ) : null}
                  </div>
                  {profile.address ? (
                    <div className="stam__addrrow">
                      <span className="stam__addr">
                        {profile.address.slice(0, 6)}...{profile.address.slice(-4)}
                      </span>
                    </div>
                  ) : null}
                </div>
              </div>
            </div>

            <nav className="stam__tabs" aria-label={suffixLabel("Profile sections", labelSuffix)}>
              {tabs.map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  className={"stam__tab" + (tab.id === activeTab ? " is-active" : "")}
                  aria-current={tab.id === activeTab ? "page" : undefined}
                >
                  {tab.label}
                  {tab.id === activeTab ? <span className="stam__tabbar" aria-hidden="true" /> : null}
                </button>
              ))}
            </nav>

            <div className="stam__body">
              <div className="stam__assetshead">
                <div className="stam__filters" role="group" aria-label="Filter assets">
                  {CATEGORY_FILTERS.map((opt) => {
                    const active = opt.value === category;
                    return (
                      <button
                        key={opt.value}
                        type="button"
                        className={"stam__chip" + (active ? " is-active" : "")}
                        aria-pressed={active}
                      >
                        <span className="stam__chipicon">{ICONS[opt.icon]}</span>
                        {opt.label}
                      </button>
                    );
                  })}
                </div>
              </div>

              {loading ? (
                <div className="stam__loadingrow stam__loadingrow--center">
                  <span className="stam__spinner" role="status" aria-label="Loading" />
                </div>
              ) : showEmpty ? (
                <div className="stam__emptybox">
                  <div className="stam__emptyicon">
                    <svg viewBox="0 0 24 24" width="56" height="56" aria-hidden="true">
                      <path d="M21.6 18.2 13 13.6V11a3 3 0 1 0-4-2.8" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
                      <path d="M11 13.6 2.4 18.2a1 1 0 0 0 .5 1.8h18.2a1 1 0 0 0 .5-1.8" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  </div>
                  <div className="stam__emptybody">
                    <h2 className="stam__emptytitle">No assets yet</h2>
                    <p className="stam__emptysub">Go to marketplace and start creating your unique look</p>
                    <a className="stam__emptycta" href="/shop">
                      Go to marketplace
                    </a>
                  </div>
                </div>
              ) : (
                <>
                  <p className="stam__count">{total} assets</p>

                  {isNames ? (
                    <div className="stam__namerow">
                      {names.map((n) => (
                        <div key={n.id} className="stam__namecard">
                          <span className="stam__namelogo" aria-hidden="true">
                            <svg viewBox="0 0 24 24" width="26" height="26">
                              <circle cx="12" cy="12" r="3.4" fill="none" stroke="currentColor" strokeWidth="1.8" />
                              <path d="M15.4 12v1.2a2.3 2.3 0 0 0 4.6 0V12a8 8 0 1 0-3.2 6.4" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
                            </svg>
                          </span>
                          <span className="stam__namelabel u-truncate">
                            {n.stem}
                            <span className="stam__namesuffix">.dcl.eth</span>
                          </span>
                          <div className="stam__nameactions">
                            <span className="stam__nameedit" role="button" tabIndex={0}>
                              Edit
                            </span>
                            <span className="stam__nametransfer" role="button" tabIndex={0}>
                              Transfer
                            </span>
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <div className="stam__grid">
                      {wearables.map((item) => (
                        <AssetCard key={item.id} item={item} />
                      ))}
                    </div>
                  )}

                  {!isNames ? (
                    <div className="stam__loadingrow stam__loadingrow--center">
                      <button type="button" className="stam__loadmore">
                        Load more
                      </button>
                    </div>
                  ) : null}
                </>
              )}
            </div>
          </div>
        </div>
      </div>
    </SitesChromeMaybe>
  );
}
