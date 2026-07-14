import type { ComponentProps } from "react";

import ManaMark from "../../atoms/ManaMark";
import { Coin } from "../../atoms/icons";
import { creditsSrLabel } from "../credits-unit";
import MkCollectionPage from "./MkCollectionPage";
import "./collectionview.css";

type MkCollectionPageProps = ComponentProps<typeof MkCollectionPage>;

const SORT_OPTIONS: { id: string; label: string }[] = [
  { id: "recently_listed", label: "Recently listed" },
  { id: "cheapest", label: "Cheapest" },
  { id: "most_expensive", label: "Most expensive" },
  { id: "recently_sold", label: "Recently sold" },
];

export type MkCollectionStats = {
  floor: string | null;
  floorCredits: string | null;
  creatorShort: string;
  itemCount: number;
  network: "ethereum" | "polygon";
};

export type MkCollectionViewProps = {
  stats?: MkCollectionStats;
  sortBy?: string;
  collection?: MkCollectionPageProps["collection"];
  items?: MkCollectionPageProps["items"];
  rootRef?: { current: HTMLDivElement | null };
  onSort?: (value: string) => void;
};

const EMPTY_STATS: MkCollectionStats = {
  floor: null,
  floorCredits: null,
  creatorShort: "",
  itemCount: 0,
  network: "polygon",
};

export default function MkCollectionView({
  stats = EMPTY_STATS,
  sortBy = "recently_listed",
  collection = undefined,
  items = [],
  rootRef = undefined,
  onSort = undefined,
}: MkCollectionViewProps) {
  const empty = !collection || items.length === 0;
  const state = empty ? "empty" : "ready";

  return (
    <div className="mkcoll" ref={rootRef}>
      <div className="mkcoll__bar">
        <div className="mkcoll__stat">
          <span className="mkcoll__statlabel">Items</span>
          <span className="mkcoll__statvalue">{stats.itemCount}</span>
        </div>
        <div className="mkcoll__stat">
          <span className="mkcoll__statlabel">Floor</span>
          <span className="mkcoll__statvalue">
            {stats.floorCredits != null ? (
              <>
                <Coin size={13} className="mkcoll__manamark" />
                {stats.floorCredits}
                <span className="u-sr-only">{creditsSrLabel(stats.floorCredits)}</span>
              </>
            ) : stats.floor != null ? (
              <>
                <ManaMark size={13} network={stats.network} className="mkcoll__manamark" />
                {stats.floor}
                <span className="u-sr-only"> MANA</span>
              </>
            ) : (
              "\u{2014}"
            )}
          </span>
        </div>
        <div className="mkcoll__stat">
          <span className="mkcoll__statlabel">Creator</span>
          <span className="mkcoll__statvalue">{stats.creatorShort || "\u{2014}"}</span>
        </div>

        <div className="mkcoll__sort">
          <label className="mkcoll__sortlabel" htmlFor="mkcoll-sort">
            Order by
          </label>
          <select
            id="mkcoll-sort"
            className="mkcoll__select"
            value={sortBy}
            onChange={(e) => onSort?.(e.target.value)}
          >
            {SORT_OPTIONS.map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      <MkCollectionPage collection={collection} items={items} state={state} />
    </div>
  );
}
