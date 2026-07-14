import { useMemo, useState } from "react";
import SitesChrome from "../frames/SitesChrome";
import { rarityColor } from "../../atoms/primitives";
import "./stprofilecreationstab.css";

type Rarity = { color: string; label: string };

const RARITY_LABEL: Record<string, string> = {
  unique: "Unique",
  mythic: "Mythic",
  exotic: "Exotic",
  legendary: "Legendary",
  epic: "Epic",
  rare: "Rare",
  uncommon: "Uncommon",
  common: "Common",
};

function rarityOf(id: string): Rarity {
  const key = id in RARITY_LABEL ? id : "common";
  return { color: rarityColor(key), label: RARITY_LABEL[key] ?? "Common" };
}

export type CreationProfile = { address: string; name?: string | null; accountUrl: string };

export type CreationItem = {
  id: string;
  name: string;
  creator: string;
  price: string | null;
  rarity: string;
  category: string | null;
  body: string | null;
  smart: boolean;
};

type TabItem = { id: string; label: string };

type Category = "wearable" | "emote";

const EMPTY_PROFILE: CreationProfile = { address: "", accountUrl: "" };

const COPY = {
  filter_wearables: "Wearables",
  filter_emotes: "Emotes",
  view_all: "View all",
  not_for_sale: "Not for sale",
  view_in_marketplace: "View in marketplace",
  buy: "Buy",
  load_more: "Load more",
  empty_member: "This account has not created anything yet.",
  empty_owner: "Items you create will appear here.",
};

const TABS: TabItem[] = [
  { id: "overview", label: "Overview" },
  { id: "creations", label: "Creations" },
  { id: "communities", label: "Communities" },
  { id: "places", label: "Places" },
  { id: "photos", label: "Photos" },
];

const PAGE_SIZE = 6;

function truncateAddress(addr: string) {
  return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
}

function formatCategoryLabel(category: string) {
  return category
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

const CheckroomGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M12 6.5a2 2 0 1 1 1.4 1.9c-.2.1-.4.4-.4.7v.8l8 4.6c.6.4 1 1 1 1.7 0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2 0-.7.4-1.3 1-1.7l8-4.6v-.8" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" strokeLinecap="round" />
  </svg>
);
const EmojiGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="1.6" />
    <circle cx="9" cy="10" r="1.1" fill="currentColor" />
    <circle cx="15" cy="10" r="1.1" fill="currentColor" />
    <path d="M8 14.5a5 5 0 0 0 8 0" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);
const ChevronGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="m9 6 6 6-6 6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const ManaGlyph = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path d="M12 2 4 12l8 10 8-10L12 2Z" fill="#a524b2" />
    <path d="M12 2 4 12h16L12 2Z" fill="#ff2d55" opacity="0.85" />
  </svg>
);
const BodyGlyph = ({ kind }: { kind: string }) => (
  <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true">
    <circle cx="10" cy="5" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
    {kind === "female" ? (
      <path d="M7 10h6l1.5 6h-9L7 10Z" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
    ) : kind === "male" ? (
      <path d="M7 9.5h6v6.5H7V9.5Z" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
    ) : (
      <path d="M7 9.5h6v3l-1 3.5H8l-1-3.5v-3Z" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
    )}
  </svg>
);
const SmartGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M13 2 4 13h6l-1 9 9-11h-6l1-9Z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
  </svg>
);
const CategoryGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M3 7l9-4 9 4-9 4-9-4Zm0 5 9 4 9-4M3 17l9 4 9-4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" strokeLinecap="round" />
  </svg>
);

function CreationCard({ item }: { item: CreationItem }) {
  const r = rarityOf(item.rarity);
  const initial = item.name.charAt(0).toUpperCase();
  const hasPrice = Boolean(item.price);
  return (
    <a
      className="stpc__cardlink"
      href={`/marketplace/${encodeURIComponent(item.id)}`}
      aria-label={item.name}
    >
      <div className="stpc__catalog">
        <div className="stpc__cardimg" style={{ backgroundColor: r.color }}>
          <span className="stpc__cardglyph" aria-hidden="true">{initial}</span>
        </div>
        <div className="stpc__cardinfo">
          <div className="stpc__cardhead">
            <span className="stpc__cardtitle">{item.name}</span>
            <span className="stpc__cardby">By {item.creator}</span>
          </div>

          {hasPrice ? (
            <div className="stpc__pricerow">
              <ManaGlyph />
              <span className="stpc__price">{item.price}</span>
            </div>
          ) : (
            <span className="stpc__owners">{COPY.not_for_sale}</span>
          )}

          <div className="stpc__badgerow">
            <span
              className="stpc__raritychip"
              style={{ background: r.color, color: "var(--rar-ink, #161518)" }}
            >
              {r.label}
            </span>
            <span className="stpc__infobadges">
              {item.category ? (
                <span className="u-tip" title={formatCategoryLabel(item.category)}>
                  <CategoryGlyph />
                </span>
              ) : null}
              {item.body ? (
                <span className="u-tip" title={item.body === "unisex" ? "Unisex" : item.body === "male" ? "For male" : "For female"}>
                  <BodyGlyph kind={item.body} />
                </span>
              ) : null}
              {item.smart ? (
                <span className="u-tip" title="Smart wearable">
                  <SmartGlyph />
                </span>
              ) : null}
            </span>
          </div>

          <div className="stpc__bottomaction">
            <span className={"stpc__buy" + (hasPrice ? "" : " stpc__buy--outlined")}>
              {hasPrice ? COPY.buy : COPY.view_in_marketplace}
            </span>
          </div>
        </div>
      </div>
    </a>
  );
}

type StProfileCreationsTabProps = {
  profile?: CreationProfile;
  tabs?: TabItem[];
  isOwnProfile?: boolean;
  loading?: boolean;
  empty?: boolean;
  wearables?: CreationItem[];
  emotes?: CreationItem[];
};

export default function StProfileCreationsTab({
  profile = EMPTY_PROFILE,
  tabs = TABS,
  isOwnProfile = false,
  loading = false,
  empty = false,
  wearables = [],
  emotes = [],
}: StProfileCreationsTabProps) {
  const [category, setCategory] = useState<Category>("wearable");
  const [visible, setVisible] = useState(PAGE_SIZE);

  const allItems = category === "wearable" ? wearables : emotes;
  const source = empty ? [] : allItems;
  const items = useMemo(() => source.slice(0, visible), [source, visible]);
  const canLoadMore = items.length < source.length;
  const initial = (profile.name || profile.address).charAt(0).toUpperCase();

  function selectCategory(next: Category) {
    if (next === category) return;
    setCategory(next);
    setVisible(PAGE_SIZE);
  }

  const header = (
    <div className="stpc__chead">
      <div className="stpc__filters">
        <button
          type="button"
          className={"stpc__chip" + (category === "wearable" ? " is-active" : "")}
          aria-pressed={category === "wearable"}
          onClick={() => selectCategory("wearable")}
        >
          <span className="stpc__chipicon"><CheckroomGlyph /></span>
          {COPY.filter_wearables}
        </button>
        <button
          type="button"
          className={"stpc__chip" + (category === "emote" ? " is-active" : "")}
          aria-pressed={category === "emote"}
          onClick={() => selectCategory("emote")}
        >
          <span className="stpc__chipicon"><EmojiGlyph /></span>
          {COPY.filter_emotes}
        </button>
      </div>
      {profile.accountUrl ? (
        <a
          className="stpc__viewall"
          href={profile.accountUrl}
          target="_blank"
          rel="noopener noreferrer"
        >
          {COPY.view_all}
          <ChevronGlyph />
        </a>
      ) : null}
    </div>
  );

  return (
    <SitesChrome active="legal" overlayNav>
      <div className="stpc">
        <div className="stpc__content">
          <div className="stpc__card">
            <div className="stpc__phead">
              <div className="stpc__identity">
                <span className="stpc__avatar" style={{ background: "#FF8362" }} aria-hidden="true">
                  <span className="stpc__avatarinitial">{initial}</span>
                </span>
                <div className="stpc__nameblock">
                  <div className="stpc__namerow">
                    <span className="stpc__name" style={{ color: "#FF8362" }}>{profile.name}</span>
                    {profile.name ? (
                      <span className="stpc__verified" style={{ background: "#FF8362" }} title="Verified">
                        <svg viewBox="0 0 24 24" width="11" height="11" aria-hidden="true">
                          <path d="m6 12 4 4 8-9" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
                        </svg>
                      </span>
                    ) : null}
                  </div>
                  {profile.address ? (
                    <div className="stpc__addrrow">
                      <span className="stpc__walleticon">
                        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
                          <path d="M3 7a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v1h1a1 1 0 0 1 1 1v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Zm14 7a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
                        </svg>
                      </span>
                      <span className="stpc__addr">{truncateAddress(profile.address)}</span>
                      <button type="button" className="stpc__copy" aria-label="Copy address">
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
                          <rect x="9" y="9" width="11" height="11" rx="2" fill="none" stroke="currentColor" strokeWidth="1.6" />
                          <path d="M5 15V5a2 2 0 0 1 2-2h8" fill="none" stroke="currentColor" strokeWidth="1.6" />
                        </svg>
                      </button>
                    </div>
                  ) : null}
                </div>
              </div>

              <div className="stpc__actions">
                <button type="button" className="stpc__cta">
                  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
                    <circle cx="9" cy="8" r="3.4" fill="none" stroke="currentColor" strokeWidth="1.7" />
                    <path d="M3.5 19a5.5 5.5 0 0 1 11 0M18 8v6M15 11h6" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
                  </svg>
                  Add friend
                </button>
                <button type="button" className="stpc__more" aria-label="More actions">
                  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
                    <circle cx="12" cy="5" r="1.6" fill="currentColor" />
                    <circle cx="12" cy="12" r="1.6" fill="currentColor" />
                    <circle cx="12" cy="19" r="1.6" fill="currentColor" />
                  </svg>
                </button>
              </div>
            </div>

            <nav className="stpc__tabs" aria-label="Profile sections">
              {tabs.map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  className={"stpc__tab" + (tab.id === "creations" ? " is-active" : "")}
                  aria-current={tab.id === "creations" ? "page" : undefined}
                >
                  {tab.label}
                  {tab.id === "creations" ? <span className="stpc__tabbar" aria-hidden="true" /> : null}
                </button>
              ))}
            </nav>

            <div className="stpc__body">
              {header}

              {loading ? (
                <div className="stpc__loadingrow">
                  <span className="stpc__spinner" role="status" aria-label="Loading" />
                </div>
              ) : items.length === 0 ? (
                <p className="stpc__emptybio">
                  {isOwnProfile ? COPY.empty_owner : COPY.empty_member}
                </p>
              ) : (
                <>
                  <div className="stpc__grid">
                    {items.map((item) => (
                      <CreationCard key={item.id} item={item} />
                    ))}
                  </div>
                  {canLoadMore ? (
                    <div className="stpc__loadmorerow">
                      <button
                        type="button"
                        className="stpc__loadmore"
                        onClick={() => setVisible((v) => v + PAGE_SIZE)}
                      >
                        {COPY.load_more}
                      </button>
                    </div>
                  ) : null}
                </>
              )}
            </div>
          </div>
        </div>
      </div>
    </SitesChrome>
  );
}
