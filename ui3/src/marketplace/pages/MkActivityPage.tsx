import type { ComponentType, ReactNode } from "react";

import MkMySalesHistory from "../components/MkMySalesHistory";
import "./activityfeed.css";

export const ACTIVITY_TYPES = ["sale", "listing", "bid"] as const;
export type MkActivityType = (typeof ACTIVITY_TYPES)[number];

export type MkActivityEntry = {
  id: string;
  kind: MkActivityType;
  rawType: string;
  price: string | null;
  network: "ethereum" | "polygon";
  from: string;
  to: string;
  timestamp: number;
  txHash: string | null;
  contractAddress: string | null;
};

const TYPE_LABELS: Record<MkActivityType, string> = {
  sale: "Sales",
  listing: "Listings",
  bid: "Bids",
};

const KIND_BADGE: Record<MkActivityType, string> = {
  sale: "Sale",
  listing: "Listing",
  bid: "Bid",
};

const RAW_TYPE_LABELS: Record<string, string> = {
  mint: "Mint",
  order: "Sale",
  sale: "Sale",
  transfer: "Transfer",
  listing: "Listing",
  public_nft_order: "Listing",
  public_item_order: "Mint listing",
  bid: "Bid",
  public_item_bid: "Bid",
};
function prettyRawType(t: string): string {
  return (
    RAW_TYPE_LABELS[t] ??
    t
      .replace(/_/g, " ")
      .replace(/\bnft\b/gi, "NFT")
      .replace(/\b\w/g, (c) => c.toUpperCase())
  );
}

type LinkComponentProps = {
  className?: string;
  to: string;
  prefetch?: "intent";
  onClick?: () => void;
  children?: ReactNode;
};

export type MkMySalesProps = {
  me: string;
  mine: MkActivityEntry[];
  manaEarned: string;
};

export type MkActivityPageProps = {
  entries?: MkActivityEntry[];
  type?: MkActivityType | "";
  page?: number;
  pageSize?: number;
  hasNext?: boolean;
  fallback?: boolean;
  salesTotal?: number;
  tradesTotal?: number;
  mySales?: MkMySalesProps;
  LinkComponent?: ComponentType<LinkComponentProps>;
  onSelectType?: (type: MkActivityType | "") => void;
  onPage?: (page: number, direction: "prev" | "next") => void;
  onEntryClick?: (entry: MkActivityEntry) => void;
};

export default function MkActivityPage({
  entries = [],
  type = "",
  page = 0,
  pageSize = 25,
  hasNext = false,
  fallback = false,
  salesTotal = 0,
  tradesTotal = 0,
  mySales = { me: "", mine: [], manaEarned: "0" },
  LinkComponent = undefined,
  onSelectType = undefined,
  onPage = undefined,
  onEntryClick = undefined,
}: MkActivityPageProps) {
  return (
    <div className="mkact">
      <header className="mkact__head">
        <h1 className="mkact__title">Activity</h1>
        <p className="mkact__sub">
          {salesTotal.toLocaleString()} settled sales · {tradesTotal.toLocaleString()} open
          trades on the Decentraland marketplace.
        </p>
      </header>

      <div className="mkact__filters" role="tablist" aria-label="Activity type">
        <FilterTab label="All" active={type === ""} onSelect={() => onSelectType?.("")} />
        {ACTIVITY_TYPES.map((t) => (
          <FilterTab
            key={t}
            label={TYPE_LABELS[t]}
            active={type === t}
            onSelect={() => onSelectType?.(t)}
          />
        ))}
      </div>

      {entries.length > 0 ? (
        <div className="mkact__tablewrap">
          <table className="mkact__table">
            <thead>
              <tr>
                <th scope="col">Type</th>
                <th scope="col">Item</th>
                <th scope="col">Price</th>
                <th scope="col">From</th>
                <th scope="col">To</th>
                <th scope="col">When</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((e) => (
                <ActivityRow key={e.id} entry={e} onClick={() => onEntryClick?.(e)} />
              ))}
            </tbody>
          </table>
        </div>
      ) : fallback ? (
        <p className="mkact__empty" role="alert">
          Couldn&apos;t load activity right now. Please try again.
        </p>
      ) : (
        <p className="mkact__empty">No activity for this filter.</p>
      )}

      <nav className="mkact__pager" aria-label="Activity pagination">
        <button
          type="button"
          className="mkact__pagebtn"
          disabled={page <= 0}
          onClick={() => onPage?.(page - 1, "prev")}
        >
          ← Newer
        </button>
        <span className="mkact__pageinfo">
          Page {page + 1} · {entries.length} shown ({pageSize}/page)
        </span>
        <button
          type="button"
          className="mkact__pagebtn"
          disabled={!hasNext}
          onClick={() => onPage?.(page + 1, "next")}
        >
          Older →
        </button>
      </nav>

      <MkMySalesHistory me={mySales.me} mine={mySales.mine} manaEarned={mySales.manaEarned} />
    </div>
  );

  function ActivityRow({
    entry,
    onClick,
  }: {
    entry: MkActivityEntry;
    onClick: () => void;
  }) {
    const detailId = entry.contractAddress
      ? `${entry.contractAddress}-0`
      : entry.id;
    const to = `/marketplace/${encodeURIComponent(detailId)}`;
    const linkBody = (
      <>
        <span className="mkact__net" data-net={entry.network}>
          {entry.network === "ethereum" ? "ETH" : "MATIC"}
        </span>
        <span className="mkact__id" title={entry.id}>
          {prettyRawType(entry.rawType)}
        </span>
      </>
    );
    return (
      <tr className="mkact__row" data-kind={entry.kind}>
        <td>
          <span className={`mkact__badge mkact__badge--${entry.kind}`}>
            {KIND_BADGE[entry.kind]}
          </span>
        </td>
        <td className="mkact__item">
          {LinkComponent ? (
            <LinkComponent
              to={to}
              prefetch="intent"
              onClick={onClick}
              className="mkact__itemlink"
            >
              {linkBody}
            </LinkComponent>
          ) : (
            <a href={to} onClick={onClick} className="mkact__itemlink">
              {linkBody}
            </a>
          )}
        </td>
        <td className="mkact__price">
          {entry.price ? (
            <>
              <span className="mkact__mana">◇</span> {entry.price}
            </>
          ) : (
            <span className="mkact__dash">—</span>
          )}
        </td>
        <td className="mkact__addr">{entry.from || "—"}</td>
        <td className="mkact__addr">{entry.to || "—"}</td>
        <td className="mkact__when">{relativeTime(entry.timestamp)}</td>
      </tr>
    );
  }
}

function FilterTab({
  label,
  active,
  onSelect,
}: {
  label: string;
  active: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      className={"mkact__tab" + (active ? " is-active" : "")}
      onClick={onSelect}
    >
      {label}
    </button>
  );
}

export function relativeTime(ts: number, now: number = Date.now()): string {
  if (!ts) return "—";
  const diff = Math.max(0, now - ts);
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  if (day < 30) return `${day}d ago`;
  const mo = Math.floor(day / 30);
  if (mo < 12) return `${mo}mo ago`;
  return `${Math.floor(mo / 12)}y ago`;
}
