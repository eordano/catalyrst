import type { CSSProperties, ReactNode } from "react";
import { useState } from "react";
import { MarketplaceChromeMaybe, type MarketplaceNavId } from "../frames/MarketplaceChrome";
import Button from "../../atoms/Button";
import ManaMark from "../../atoms/ManaMark";
import { Coin } from "../../atoms/icons";
import { creditsSrLabel } from "../credits-unit";
import "./mkassetpage.css";

const COPY = {
  back: "Back",
  description: "Description",
  no_description: "This item has no description.",
  read_more: "READ MORE",
  read_less: "READ LESS",
  owner: "Owner",
  collection: "Collection",
  price: "PRICE",
  issue_number: "ISSUE NUMBER",
  buy: "Buy",
  add_to_cart: "Add to cart",
  make_offer: "Make an offer",
  your_offer: "Your offer",
  other_available_listings: "Other available listings",
  cheapest: "Cheapest",
  newest: "Newest",
  oldest: "Oldest",
  issue_asc: "Issue number: Low to high",
  issue_desc: "Issue number: High to low",
  th_owner: "Owner",
  th_published: "Published",
  th_expires: "Expiration date",
  th_issue: "Issue number",
  th_price: "Price",
  view_listing: "View listing",
  no_listings: "There are no listings for this item yet.",
  transaction_history: "Transaction history",
};

type NftOrder = { price: string; credits?: string | null; issuedId: number; expiresLabel: string };
type AssetNft = {
  name: string;
  issuedId?: number;
  category?: string;
  rarity: string;
  bodyShape?: string;
  isSmart?: boolean;
  network?: string;
  description?: string;
  image?: string;
  owner: { address: string; name?: string };
  collection: { name: string; address: string };
  order?: NftOrder | null;
};

type AssetListing = {
  owner: string;
  name?: string;
  published: string;
  expires: string;
  issued: number;
  price: string;
  credits?: string | null;
  listed: boolean;
};

function formatAmount(value: string): string {
  const n = Number(value);
  return Number.isFinite(n) ? n.toLocaleString() : value;
}

const AssetGlyph = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="0.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M9 3l3 2 3-2 4 3-2 3-2-1v11H9V8L7 9 5 6z" />
  </svg>
);

function RarityBadge({ rarity }: { rarity: string }) {
  return (
    <span className="mkassetpage__badge mkassetpage__badge--rarity" style={{ "--chip": `var(--rar-${rarity})` } as CSSProperties}>
      {rarity}
    </span>
  );
}

function MetaBadge({ icon, children }: { icon?: ReactNode; children?: ReactNode }) {
  return (
    <span className="mkassetpage__badge mkassetpage__badge--meta">
      {icon ? <span className="mkassetpage__badgeicon">{icon}</span> : null}
      {children}
    </span>
  );
}

function shortAddr(a: string) {
  return a.length > 12 ? `${a.slice(0, 6)}\u{2026}${a.slice(-4)}` : a;
}
function avatarHue(seed: string) {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) % 360;
  return h;
}

const EMPTY_NFT: AssetNft = {
  name: "",
  rarity: "",
  owner: { address: "" },
  collection: { name: "", address: "" },
  order: null,
};

const SORT_OPTIONS = [
  { value: "cheapest", text: COPY.cheapest },
  { value: "newest", text: COPY.newest },
  { value: "oldest", text: COPY.oldest },
  { value: "issue_asc", text: COPY.issue_asc },
  { value: "issue_desc", text: COPY.issue_desc },
];

function BuyNFTBox({
  nft,
  buyDisabledReason,
  onBuy,
  onAddToCart,
  addToCartLabel,
  onMakeOffer,
}: {
  nft: AssetNft;
  buyDisabledReason?: string;
  onBuy?: () => void;
  onAddToCart?: () => void;
  addToCartLabel?: string;
  onMakeOffer?: () => void;
}) {
  const order = nft.order;
  return (
    <div className="mkassetpage__buybox">
      {order ? (
        <>
          <div className="mkassetpage__buyinfo">
            <div className="mkassetpage__buycol">
              <span className="mkassetpage__buytitle">{COPY.price}</span>
              <div className="mkassetpage__price">
                {order.credits != null && order.credits !== "0" ? (
                  <>
                    <span className="mkassetpage__pricemana"><Coin size={28} /></span>
                    <span className="mkassetpage__priceval">{formatAmount(order.credits)}<span className="u-sr-only">{creditsSrLabel(order.credits)}</span></span>
                    {order.price !== "0" && (
                      <span className="mkassetpage__pricemananote">Listed at {formatAmount(order.price)} MANA</span>
                    )}
                  </>
                ) : Number(order.price) === 0 || order.credits === "0" ? (
                  <span className="mkassetpage__priceval">Free</span>
                ) : (
                  <>
                    <span className="mkassetpage__pricemana"><ManaMark size={28} /></span>
                    <span className="mkassetpage__priceval">{Number(order.price).toLocaleString()}<span className="u-sr-only"> MANA</span></span>
                  </>
                )}
              </div>
            </div>
            <div className="mkassetpage__buycol">
              <span className="mkassetpage__buytitle">{COPY.issue_number}</span>
              <div className="mkassetpage__issue">{order.issuedId ? `#${order.issuedId}` : "\u{2014}"}</div>
            </div>
          </div>

          <Button
            variant="primary"
            className="mkassetpage__buybtn"
            disabled={!!buyDisabledReason}
            onClick={buyDisabledReason ? undefined : onBuy}
          >
            {COPY.buy}
          </Button>
          {buyDisabledReason ? (
            <span className="mkassetpage__buynote" role="note">
              {buyDisabledReason}
            </span>
          ) : null}
          {onAddToCart ? (
            <Button variant="secondary" className="mkassetpage__buybtn mkassetpage__buybtn--cart" onClick={onAddToCart}>
              {addToCartLabel ?? COPY.add_to_cart}
            </Button>
          ) : null}
          {onMakeOffer ? (
            <Button variant="secondary" className="mkassetpage__buybtn mkassetpage__offerbtn" onClick={onMakeOffer}>{COPY.make_offer}</Button>
          ) : null}

          <span className="mkassetpage__expires">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" />
            </svg>
            {order.expiresLabel}.
          </span>
        </>
      ) : (
        <>
          <div className="mkassetpage__buyinfo">
            <div className="mkassetpage__buycol">
              <span className="mkassetpage__buytitle">{COPY.price}</span>
              <div className="mkassetpage__issue">Not for sale</div>
            </div>
            <div className="mkassetpage__buycol">
              <span className="mkassetpage__buytitle">{COPY.issue_number}</span>
              <div className="mkassetpage__issue">{nft.issuedId ? `#${nft.issuedId}` : "\u{2014}"}</div>
            </div>
          </div>
          {onMakeOffer ? (
            <Button variant="secondary" className="mkassetpage__buybtn mkassetpage__offerbtn" onClick={onMakeOffer}>{COPY.make_offer}</Button>
          ) : null}
        </>
      )}
    </div>
  );
}

function StatTile({ label, children }: { label: ReactNode; children?: ReactNode }) {
  return (
    <div className="mkassetpage__stat">
      <span className="mkassetpage__statlabel">{label}</span>
      {children}
    </div>
  );
}

function ListingsTable({
  listings,
  sortBy,
  onSort,
  onViewListing,
}: {
  listings: AssetListing[];
  sortBy: string;
  onSort: (value: string) => void;
  onViewListing?: (index: number) => void;
  chrome?: boolean;
}) {
  return (
    <div className="mkassetpage__tablecard">
      <div className="mkassetpage__tabletop">
        <div className="mkassetpage__tabs">
          <button type="button" className="mkassetpage__tab is-active">{COPY.other_available_listings}</button>
        </div>
        <label className="mkassetpage__sort">
          <span>Order by</span>
          <select value={sortBy} onChange={(e) => onSort(e.target.value)}>
            {SORT_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>{o.text}</option>
            ))}
          </select>
        </label>
      </div>

      {listings.length ? (
        <table className="mkassetpage__table">
          <thead>
            <tr>
              <th>{COPY.th_owner}</th>
              <th>{COPY.th_published}</th>
              <th>{COPY.th_expires}</th>
              <th>{COPY.th_issue}</th>
              <th className="mkassetpage__th--right">{COPY.th_price}</th>
            </tr>
          </thead>
          <tbody>
            {listings.map((l, i) => (
              <tr key={l.issued}>
                <td>
                  <span className="mkassetpage__profile">
                    <span className="u-avatar" style={{ "--sz": "28px", "--hue": avatarHue(l.owner) } as CSSProperties} />
                    {l.name || shortAddr(l.owner)}
                  </span>
                </td>
                <td className="mkassetpage__muted">{l.published}</td>
                <td className="mkassetpage__muted">{l.expires}</td>
                <td>
                  <span className="mkassetpage__issuecell">
                    {l.listed ? <span className="mkassetpage__listed">Listed</span> : null}
                    #{l.issued}
                  </span>
                </td>
                <td className="mkassetpage__th--right">
                  <span className="mkassetpage__rowprice">
                    {l.credits != null ? (
                      <>
                        <span className="mkassetpage__rowmana"><Coin size={16} /></span>
                        {formatAmount(l.credits)}
                        <span className="u-sr-only">{creditsSrLabel(l.credits)}</span>
                        <span className="mkassetpage__rowmananote">({Number(l.price).toLocaleString()} MANA)</span>
                      </>
                    ) : (
                      <>
                        <span className="mkassetpage__rowmana"><ManaMark size={16} /></span>
                        {Number(l.price).toLocaleString()}
                        <span className="u-sr-only"> MANA</span>
                      </>
                    )}
                    <Button
                      variant="secondary"
                      className="mkassetpage__viewlisting"
                      disabled={!!onViewListing && !l.listed}
                      onClick={onViewListing ? () => onViewListing(i) : undefined}
                    >
                      {COPY.view_listing}
                    </Button>
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <div className="mkassetpage__emptytable">
          <div className="mkassetpage__emptyicon">
            <svg viewBox="0 0 24 24" width="32" height="32" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <rect x="3" y="4" width="18" height="16" rx="2" /><path d="M3 9h18M8 14h8" />
            </svg>
          </div>
          <span>{COPY.no_listings}</span>
        </div>
      )}
    </div>
  );
}

export default function MkAssetPage({
  nft = EMPTY_NFT,
  listings = [],
  emptyListings = false,
  favorited = false,
  banner,
  tryOn,
  buyDisabledReason,
  onToggleFavorite,
  onBuy,
  onAddToCart,
  addToCartLabel,
  onMakeOffer,
  onBack,
  onViewListing,
  chrome = true,
}: {
  nft?: AssetNft;
  listings?: AssetListing[];
  emptyListings?: boolean;
  favorited?: boolean;
  banner?: ReactNode;
  tryOn?: ReactNode;
  buyDisabledReason?: string;
  onToggleFavorite?: () => void;
  onBuy?: () => void;
  onAddToCart?: () => void;
  addToCartLabel?: string;
  onMakeOffer?: () => void;
  onBack?: () => void;
  onViewListing?: (index: number) => void;
  chrome?: boolean;
}) {
  const [tab, setTab] = useState<MarketplaceNavId>("collectibles");
  const [showMore, setShowMore] = useState(false);
  const [sortBy, setSortBy] = useState("cheapest");
  const [view, setView] = useState<"image" | "tryon">("image");

  const MAX = 148;
  const desc = nft.description || "";
  const hasMore = desc.length > MAX;
  const descText = hasMore && !showMore ? `${desc.slice(0, MAX)}...` : desc;

  return (
    <MarketplaceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
      <div className="mkassetpage">
        <div className="mkassetpage__page">
          {banner}
          <button type="button" className="mkassetpage__back" onClick={onBack}>
            <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M12 4l-6 6 6 6" />
            </svg>
            {COPY.back}
          </button>

          <div className="mkassetpage__cols">
          <div className="mkassetpage__image">
            <div className="u-rar-bg" style={{ "--rb": `var(--rar-bg-${nft.rarity})` } as CSSProperties} />
            {tryOn ? (
              <div className="mkassetpage__viewtoggle" role="group" aria-label="Preview mode">
                <button
                  type="button"
                  aria-pressed={view === "image"}
                  className={"mkassetpage__viewtab" + (view === "image" ? " is-active" : "")}
                  onClick={() => setView("image")}
                >
                  Image
                </button>
                <button
                  type="button"
                  aria-pressed={view === "tryon"}
                  className={"mkassetpage__viewtab" + (view === "tryon" ? " is-active" : "")}
                  onClick={() => setView("tryon")}
                >
                  Try on
                </button>
              </div>
            ) : null}
            {tryOn && view === "tryon" ? (
              <div className="mkassetpage__tryon">{tryOn}</div>
            ) : nft.image ? (
              <img className="mkassetpage__img" src={nft.image} alt={nft.name} />
            ) : (
              <div className="mkassetpage__imagefig"><AssetGlyph /></div>
            )}
            {onToggleFavorite ? (
              <button
                type="button"
                className={"mkassetpage__fav" + (favorited ? " is-on" : "")}
                aria-label={favorited ? "Remove from favorites" : "Add to favorites"}
                aria-pressed={favorited}
                onClick={onToggleFavorite}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M12 20.5l-1.35-1.2C6.4 15.5 3.5 12.9 3.5 9.7 3.5 7.3 5.4 5.5 7.75 5.5c1.35 0 2.65.63 3.5 1.63A4.66 4.66 0 0 1 14.75 5.5C17.1 5.5 19 7.3 19 9.7c0 3.2-2.9 5.8-7.15 9.6z" />
                </svg>
              </button>
            ) : null}
          </div>

          <div className="mkassetpage__side">
            <div className="mkassetpage__info">
              <div>
                <h1 className="mkassetpage__title">
                  {nft.name} {nft.issuedId ? <span className="mkassetpage__issued">#{nft.issuedId}</span> : null}
                </h1>
                <div className="mkassetpage__badges">
                  <RarityBadge rarity={nft.rarity} />
                  {nft.category ? <MetaBadge>{String(nft.category).replace(/[_-]+/g, " ")}</MetaBadge> : null}
                  {nft.bodyShape ? <MetaBadge>{nft.bodyShape}</MetaBadge> : null}
                  {nft.isSmart ? (
                    <MetaBadge
                      icon={
                        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                          <path d="M13 2 3 14h7l-1 8 10-12h-7l1-8z" />
                        </svg>
                      }
                    >
                      Smart wearable
                    </MetaBadge>
                  ) : null}
                </div>
              </div>

              <div className="mkassetpage__actions">
                <BuyNFTBox
                  nft={nft}
                  buyDisabledReason={buyDisabledReason}
                  onBuy={onBuy}
                  onAddToCart={onAddToCart}
                  addToCartLabel={addToCartLabel}
                  onMakeOffer={onMakeOffer}
                />
              </div>

              <div className="mkassetpage__attrs">
                <div className="mkassetpage__attrcol">
                  <h2 className="mkassetpage__sub">{COPY.description}</h2>
                  <p className="mkassetpage__desc">{desc ? descText : COPY.no_description}</p>
                  {hasMore ? (
                    <button type="button" className="mkassetpage__readmore" onClick={() => setShowMore((v) => !v)}>
                      {showMore ? COPY.read_less : COPY.read_more}
                    </button>
                  ) : null}
                </div>
              </div>

              <div className="mkassetpage__attrs">
                <div className="mkassetpage__attrcol">
                  <StatTile label={COPY.owner}>
                    <span className="mkassetpage__profile mkassetpage__profile--lg">
                      <span className="u-avatar" style={{ "--sz": "34px", "--hue": avatarHue(nft.owner.address) } as CSSProperties} />
                      {nft.owner.name || shortAddr(nft.owner.address)}
                    </span>
                  </StatTile>
                </div>
                <div className="mkassetpage__attrcol">
                  <StatTile label={COPY.collection}>
                    <span className="mkassetpage__profile mkassetpage__profile--lg">
                      <span className="mkassetpage__collimg" style={{ "--hue": avatarHue(nft.collection.address) } as CSSProperties} />
                      {nft.collection.name}
                    </span>
                  </StatTile>
                </div>
              </div>
            </div>

          </div>
          </div>

          <div className="mkassetpage__below">
            <ListingsTable
              listings={emptyListings ? [] : listings}
              sortBy={sortBy}
              onSort={setSortBy}
              onViewListing={onViewListing}
            />
          </div>
        </div>
      </div>
    </MarketplaceChromeMaybe>
  );
}
