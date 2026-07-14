import { useMemo, useState, type ReactNode } from "react";
import Spinner from "../../atoms/Spinner";
import "./chcollectiondetail.css";
import { ChevronLeft } from "../../atoms/icons";

type Rarity =
  | "common"
  | "uncommon"
  | "rare"
  | "epic"
  | "legendary"
  | "mythic"
  | "unique"
  | "exotic";
type ItemStatus = "ready" | "not_ready" | "published" | "under_review" | "unsynced";
type StatusIcon = "cloud" | "warn" | "check" | "clock" | "alert";

export type ChCollection = {
  name: string;
  status: string | null;
  isPublished: boolean;
  isApproved: boolean;
  isOnSale: boolean;
  isLocked: boolean;
  urn?: string | null;
  contractAddress?: string | null;
};

export type ChCollectionItem = {
  id: string;
  name: string;
  rarity: Rarity;
  category: string;
  price: string | null;
  supply: string | null;
  status: ItemStatus;
  hue: number;
  smart?: boolean;
  playMode?: "loop" | "simple";
};

const RARITY: Record<Rarity, { label: string; token: string }> = {
  common: { label: "Common", token: "--rar-common" },
  uncommon: { label: "Uncommon", token: "--rar-uncommon" },
  rare: { label: "Rare", token: "--rar-rare" },
  epic: { label: "Epic", token: "--rar-epic" },
  legendary: { label: "Legendary", token: "--rar-legendary" },
  mythic: { label: "Mythic", token: "--rar-mythic" },
  unique: { label: "Unique", token: "--rar-unique" },
  exotic: { label: "Exotic", token: "--rar-exotic" },
};

const STATUS: Record<ItemStatus, { label: string; icon: StatusIcon; cls: string }> = {
  ready: { label: "Ready to submit", icon: "cloud", cls: "is-ready" },
  not_ready: { label: "Not ready", icon: "warn", cls: "is-notready" },
  published: { label: "Published", icon: "check", cls: "is-published" },
  under_review: { label: "Under Review", icon: "clock", cls: "is-review" },
  unsynced: { label: "Unsynced", icon: "alert", cls: "is-unsynced" },
};

const EMPTY_COLLECTION: ChCollection = {
  name: "",
  status: null,
  isPublished: false,
  isApproved: false,
  isOnSale: false,
  isLocked: false,
};

function StatusGlyph({ icon }: { icon: StatusIcon }) {
  const glyphs: Record<StatusIcon, ReactNode> = {
    cloud: <path d="M5 11.5a3 3 0 0 1 .4-6 4 4 0 0 1 7.6 1.2 2.6 2.6 0 0 1-.5 5.1H5z M8 9.5V5.5m0 0L6.3 7.2M8 5.5l1.7 1.7" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />,
    warn: <><circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.3" /><path d="M8 4.6v4.2M8 11v.05" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" /></>,
    check: <><circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.3" /><path d="M5.3 8.2l1.9 1.9 3.6-3.9" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" /></>,
    clock: <><circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.3" /><path d="M8 4.8V8l2.4 1.4" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" /></>,
    alert: <><path d="M8 2.2l6 10.8H2L8 2.2z" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" /><path d="M8 6.4v3.1M8 11.4v.05" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" /></>,
  };
  const p = glyphs[icon];
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">{p}</svg>
  );
}

const PencilGlyph = () => (
  <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
    <path d="M11 2.5l2.5 2.5L6 12.5l-3 .5.5-3L11 2.5z" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
  </svg>
);
const CubeGlyph = () => (
  <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
    <path d="M8 1.5l6 3.25v6.5L8 14.5 2 11.25v-6.5L8 1.5z" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
    <path d="M2 4.75L8 8l6-3.25M8 8v6.5" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
  </svg>
);
const PlusGlyph = () => (
  <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
    <path d="M8 3v10M3 8h10" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);
const JumpGlyph = () => (
  <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
    <path d="M6 3h7v7M13 3L6.5 9.5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M11 9.5v3a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1h3" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
  </svg>
);
const WearableTabGlyph = () => (
  <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
    <path d="M6 2L3 4v3l1.5.5V14h7V7.5L13 7V4l-3-2-2 1.5L6 2z" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
  </svg>
);
const EmoteTabGlyph = () => (
  <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
    <circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.2" />
    <path d="M5.6 9.2a2.8 2.8 0 0 0 4.8 0" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    <circle cx="6" cy="6.2" r=".8" fill="currentColor" /><circle cx="10" cy="6.2" r=".8" fill="currentColor" />
  </svg>
);
const DotsGlyph = () => (
  <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
    <circle cx="3" cy="8" r="1.4" fill="currentColor" />
    <circle cx="8" cy="8" r="1.4" fill="currentColor" />
    <circle cx="13" cy="8" r="1.4" fill="currentColor" />
  </svg>
);

function RarityBadge({ rarity }: { rarity: Rarity }) {
  const r = RARITY[rarity];
  if (!r) return null;
  return (
    <span className="bdcollectiondetail__rarity" style={{ background: `var(${r.token})` }}>
      {r.label}
    </span>
  );
}

function PriceCell({ price }: { price: string | null }) {
  if (!price) return <span className="bdcollectiondetail__setprice">SET PRICE</span>;
  return (
    <span className="bdcollectiondetail__mana">
      <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true" className="bdcollectiondetail__manaicon">
        <path d="M8 1.6L13 8 8 14.4 3 8 8 1.6z M8 4.4L5 8l3 3.6L11 8 8 4.4z" fill="currentColor" />
      </svg>
      {price}
    </span>
  );
}

function CollectionItem({
  item,
  showPlayMode,
  showPrice,
  showSupply,
  isEmote,
  onOpen,
}: {
  item: ChCollectionItem;
  showPlayMode: boolean;
  showPrice: boolean;
  showSupply: boolean;
  isEmote: boolean;
  onOpen?: (item: ChCollectionItem) => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const st = STATUS[item.status];
  const nameContent = (
    <>
      <span
        className="bdcollectiondetail__itemimg"
        style={{ background: `linear-gradient(135deg, hsl(${item.hue} 60% 40%), hsl(${(item.hue + 40) % 360} 55% 24%))` }}
      />
      <span className="bdcollectiondetail__itemname u-truncate" title={item.name}>
        {item.name}
      </span>
      {item.smart ? <span className="bdcollectiondetail__smartbadge">SMART</span> : null}
    </>
  );
  return (
    <tr className="bdcollectiondetail__row" data-item-id={item.id}>
      <td className="bdcollectiondetail__cell bdcollectiondetail__namecell">
        {onOpen ? (
          <button
            type="button"
            className="bdcollectiondetail__itemopen"
            onClick={() => onOpen(item)}
            style={{
              display: "flex",
              alignItems: "center",
              padding: 0,
              border: "none",
              background: "none",
              color: "inherit",
              font: "inherit",
              textAlign: "left",
              cursor: "pointer",
            }}
          >
            {nameContent}
          </button>
        ) : (
          nameContent
        )}
      </td>
      <td className="bdcollectiondetail__cell">
        <RarityBadge rarity={item.rarity} />
      </td>
      <td className="bdcollectiondetail__cell bdcollectiondetail__muted">
        {item.category.replace(/_/g, " ")}
      </td>
      {showPlayMode ? (
        <td className="bdcollectiondetail__cell bdcollectiondetail__muted">
          {isEmote ? (item.playMode === "loop" ? "Loop" : "Single play") : ""}
        </td>
      ) : null}
      {showPrice ? (
        <td className="bdcollectiondetail__cell">
          <PriceCell price={item.price} />
        </td>
      ) : null}
      {showSupply ? (
        <td className="bdcollectiondetail__cell bdcollectiondetail__muted">
          {item.supply ? item.supply : "0/100"}
        </td>
      ) : null}
      <td className="bdcollectiondetail__cell">
        <span className={"bdcollectiondetail__status " + (st?.cls ?? "")}>
          {st && <StatusGlyph icon={st.icon} />}
          {st?.label}
        </span>
      </td>
      <td className="bdcollectiondetail__cell bdcollectiondetail__menucell">
        <button
          type="button"
          className="bdcollectiondetail__rowmenu"
          aria-label="Item options"
          aria-expanded={menuOpen}
          onClick={() => setMenuOpen((v) => !v)}
        >
          <DotsGlyph />
        </button>
        {menuOpen ? (
          <ul className="bdcollectiondetail__dropdown" role="menu">
            <li
              role="menuitem"
              className={onOpen ? "" : "is-disabled"}
              aria-disabled={onOpen ? undefined : true}
              onClick={
                onOpen
                  ? () => {
                      setMenuOpen(false);
                      onOpen(item);
                    }
                  : undefined
              }
            >
              Open in editor
            </li>
            <li role="menuitem" className="is-disabled" aria-disabled title="Not available on this realm yet">
              See in Decentraland
            </li>
            <li role="menuitem" className="is-disabled" aria-disabled title="Not available on this realm yet">
              Move to another collection
            </li>
            <li role="menuitem" className="bdcollectiondetail__divider is-disabled" aria-disabled title="Not available on this realm yet">
              Reset changes
            </li>
            <li role="menuitem" className="is-danger is-disabled" aria-disabled title="Not available on this realm yet">
              Delete item
            </li>
          </ul>
        ) : null}
      </td>
    </tr>
  );
}

type ChCollectionDetailProps = {
  collection?: ChCollection;
  wearables?: ChCollectionItem[];
  emotes?: ChCollectionItem[];
  loading?: boolean;
  initialItemType?: "wearable" | "emote";
  bare?: boolean;
  onBack?: () => void;
  onItemTypeChange?: (type: "wearable" | "emote") => void;
  onItemOpen?: (item: ChCollectionItem) => void;
  onPublish?: () => void;
  onAddItems?: () => void;
};

export default function ChCollectionDetail({
  collection = EMPTY_COLLECTION,
  wearables = [],
  emotes = [],
  loading = false,
  initialItemType = "wearable",
  onBack = undefined,
  onItemTypeChange = undefined,
  onItemOpen = undefined,
  onPublish = undefined,
  onAddItems = undefined,
}: ChCollectionDetailProps) {
  collection = collection ?? EMPTY_COLLECTION;
  const [itemType, setItemType] = useState(
    initialItemType === "emote" ? "emote" : "wearable",
  );
  const [menuOpen, setMenuOpen] = useState(false);

  const hasWearables = wearables.length > 0;
  const hasEmotes = emotes.length > 0;
  const showTabs = hasWearables && hasEmotes;
  const hasOnlyEmotes = hasEmotes && !hasWearables;

  const activeType = showTabs ? itemType : hasOnlyEmotes ? "emote" : "wearable";
  const isEmoteView = activeType === "emote";
  const items = isEmoteView ? emotes : wearables;
  const isEmpty = !hasWearables && !hasEmotes;

  const showPrice = collection.isPublished;
  const showSupply = collection.isPublished && collection.isApproved;
  const showPlayMode = isEmoteView || hasOnlyEmotes;

  const showUnsyncedNotice = useMemo(
    () => collection.isApproved && collection.status === "unsynced",
    [collection]
  );

  const actionButtons = (
    <>
      <button
        type="button"
        className="bdcollectiondetail__action"
        disabled
        title="Not available on this realm yet"
      >
        <JumpGlyph /> See in Decentraland
      </button>
      <button
        type="button"
        className="bdcollectiondetail__action"
        disabled
        title="Not available on this realm yet"
      >
        <CubeGlyph /> Preview in Editor
      </button>
      {!collection.isPublished && !isEmpty ? (
        <button
          type="button"
          className="bdcollectiondetail__action"
          disabled={collection.isLocked || !onAddItems}
          title={onAddItems ? undefined : "Not available here"}
          onClick={onAddItems}
        >
          <PlusGlyph /> Add Items
        </button>
      ) : null}
    </>
  );

  if (loading) {
    const loadingBody = (
      <div className="bdcollectiondetail bdcollectiondetail--loading">
        <Spinner size={48} />
      </div>
    );
    return loadingBody;
  }

  const body = (
      <div className="bdcollectiondetail">
        <div className="bdcollectiondetail__container">
          {onBack ? (
            <button
              type="button"
              className="bdcollectiondetail__back"
              aria-label="Back"
              onClick={onBack}
            >
              <ChevronLeft size={18} />
            </button>
          ) : null}

          <div className="bdcollectiondetail__header">
            <div className="bdcollectiondetail__namegroup">
              {collection.isPublished && collection.status ? (
                <span
                  className={"bdcollectiondetail__statusdot is-" + collection.status}
                  title={collection.status}
                />
              ) : null}
              <h1 className="bdcollectiondetail__name u-truncate">{collection.name}</h1>
              {!collection.isLocked && !collection.isPublished ? (
                <span className="bdcollectiondetail__editname" aria-hidden="true">
                  <PencilGlyph />
                </span>
              ) : null}
              {collection.isOnSale ? <span className="bdcollectiondetail__onsale">On Sale</span> : null}
            </div>

            <div className="bdcollectiondetail__headeractions">
              {collection.isPublished && collection.isApproved ? (
                <button
                  type="button"
                  className="bdcollectiondetail__action"
                  disabled
                  title="Not available on this realm yet"
                >
                  Mint Items
                </button>
              ) : null}
              {!(collection.isPublished && collection.isApproved) ? (
                <button
                  type="button"
                  className="bdcollectiondetail__publish"
                  disabled={!onPublish || isEmpty}
                  title={
                    !onPublish
                      ? "Not available here"
                      : isEmpty
                        ? "Add at least one item before publishing"
                        : undefined
                  }
                  onClick={onPublish}
                >
                  Publish Collection
                </button>
              ) : null}
              <div className="bdcollectiondetail__ctxwrap">
                <button
                  type="button"
                  className="bdcollectiondetail__ctxbtn"
                  aria-label="Collection options"
                  aria-expanded={menuOpen}
                  onClick={() => setMenuOpen((v) => !v)}
                >
                  <DotsGlyph />
                </button>
                {menuOpen ? (
                  <ul className="bdcollectiondetail__dropdown is-right" role="menu">
                    {!collection.isLocked && !collection.isPublished ? (
                      <>
                        <li role="menuitem" className="is-disabled" aria-disabled title="Not available on this realm yet">
                          Add Existing Item
                        </li>
                        <li role="menuitem" className="is-danger is-disabled" aria-disabled title="Not available on this realm yet">
                          Delete
                        </li>
                      </>
                    ) : null}
                    {collection.isPublished ? (
                      <>
                        <li role="menuitem" className="is-disabled" aria-disabled title="Not available on this realm yet">
                          Collaborators
                        </li>
                        <li role="menuitem" className="is-disabled" aria-disabled title="Not available on this realm yet">
                          Minters
                        </li>
                      </>
                    ) : null}
                    <li
                      role="menuitem"
                      className={"bdcollectiondetail__divider" + (collection.urn ? "" : " is-disabled")}
                      aria-disabled={collection.urn ? undefined : true}
                      onClick={
                        collection.urn
                          ? () => {
                              void navigator.clipboard?.writeText(collection.urn ?? "");
                              setMenuOpen(false);
                            }
                          : undefined
                      }
                    >
                      Copy URN
                    </li>
                    <li
                      role="menuitem"
                      className={
                        collection.isPublished && collection.contractAddress ? "" : "is-disabled"
                      }
                      aria-disabled={
                        collection.isPublished && collection.contractAddress ? undefined : true
                      }
                      onClick={
                        collection.isPublished && collection.contractAddress
                          ? () => {
                              void navigator.clipboard?.writeText(collection.contractAddress ?? "");
                              setMenuOpen(false);
                            }
                          : undefined
                      }
                    >
                      Copy address
                    </li>
                  </ul>
                ) : null}
              </div>
            </div>
          </div>

          {showUnsyncedNotice ? (
            <div className="bdcollectiondetail__notice">
              <span className="bdcollectiondetail__noticeicon" aria-hidden="true">
                <svg viewBox="0 0 36 36" width="32" height="32">
                  <path d="M18 4l15 27H3L18 4z" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinejoin="round" />
                  <path d="M18 14v8M18 26v.05" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" />
                </svg>
              </span>
              <div className="bdcollectiondetail__noticemsg">
                <h2 className="bdcollectiondetail__noticetitle">There are unsynced items in this collection</h2>
                <p className="bdcollectiondetail__noticetext">
                  One or more items have been modified after this collection was approved.<br />
                  These updates need to be published and reviewed again by the curation commitee.
                </p>
              </div>
              <button
                type="button"
                className="bdcollectiondetail__publish"
                disabled={!onPublish}
                title={onPublish ? undefined : "Not available here"}
                onClick={onPublish}
              >
                Publish Updates
              </button>
            </div>
          ) : null}

          {showTabs ? (
            <div className="bdcollectiondetail__tabs">
              <button
                type="button"
                className={"bdcollectiondetail__tab" + (itemType === "wearable" ? " is-active" : "")}
                onClick={() => {
                  setItemType("wearable");
                  if (itemType !== "wearable") onItemTypeChange?.("wearable");
                }}
              >
                <WearableTabGlyph /> Wearables
              </button>
              <button
                type="button"
                className={"bdcollectiondetail__tab" + (itemType === "emote" ? " is-active" : "")}
                onClick={() => {
                  setItemType("emote");
                  if (itemType !== "emote") onItemTypeChange?.("emote");
                }}
              >
                <EmoteTabGlyph /> Emotes
              </button>
              <div className="bdcollectiondetail__tabactions">{actionButtons}</div>
            </div>
          ) : !isEmpty ? (
            <div className="bdcollectiondetail__soloactions">{actionButtons}</div>
          ) : null}

          {!isEmpty ? (
            <table className="bdcollectiondetail__table">
              <thead>
                <tr>
                  <th>Item</th>
                  <th>Rarity</th>
                  <th>Category</th>
                  {showPlayMode ? <th>Play Mode</th> : null}
                  {showPrice ? <th>Price</th> : null}
                  {showSupply ? <th>Supply</th> : null}
                  <th>Status</th>
                  <th><span className="u-sr-only">Actions</span></th>
                </tr>
              </thead>
              <tbody>
                {items.map((item) => (
                  <CollectionItem
                    key={item.id}
                    item={item}
                    isEmote={isEmoteView}
                    showPlayMode={showPlayMode}
                    showPrice={showPrice}
                    showSupply={showSupply}
                    onOpen={onItemOpen}
                  />
                ))}
              </tbody>
            </table>
          ) : (
            <div className="bdcollectiondetail__empty">
              <div className="bdcollectiondetail__sparkles" aria-hidden="true">
                <svg viewBox="0 0 54 56" width="54" height="56">
                  <path d="M27 6l3.5 9.5L40 19l-9.5 3.5L27 32l-3.5-9.5L14 19l9.5-3.5L27 6z" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
                  <path d="M44 36l1.6 4.4L50 42l-4.4 1.6L44 48l-1.6-4.4L38 42l4.4-1.6L44 36z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
                </svg>
              </div>
              <div className="bdcollectiondetail__emptybody">
                <span className="bdcollectiondetail__emptytitle">Looking good!</span>
                <br />
                Now you can start adding items to your collection
                <br />
                You will not be able to add or remove items after publishing your collection.
                <br />
                <button
                  type="button"
                  className="bdcollectiondetail__emptybtn"
                  disabled={collection.isLocked || !onAddItems}
                  title={onAddItems ? undefined : "Not available here"}
                  onClick={onAddItems}
                >
                  Add Items
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
  );

  return body;
}
