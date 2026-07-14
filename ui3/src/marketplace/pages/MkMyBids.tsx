import type { CSSProperties, ReactNode } from "react";
import { useState } from "react";
import { MarketplaceChromeMaybe, type MarketplaceNavId } from "../frames/MarketplaceChrome";
import Spinner from "../../atoms/Spinner";
import ManaMark from "../../atoms/ManaMark";
import Button from "../../atoms/Button";
import "./mkmybids.css";

type StyleVars = CSSProperties & Record<`--${string}`, string | number>;
const styleVars = (v: StyleVars): CSSProperties => v;

type BidRole = "seller" | "bidder";

type Bid = {
  id: string;
  name: string;
  bidder: string;
  hue: number;
  tile: string;
  created: string;
  price: string;
  timeLeft: string;
  warning?: string | null;
};

const RAIL = [
  {
    head: "ASSETS",
    items: [
      { id: "wearables", label: "Wearables" },
      { id: "emotes", label: "Emotes" },
      { id: "names", label: "Names" },
      { id: "land", label: "Land" },
      { id: "collections", label: "Collections" },
    ],
  },
  {
    head: "STORE",
    items: [
      { id: "on_sale", label: "On Sale" },
      { id: "on_rent", label: "On Rent" },
      { id: "sales", label: "Sales" },
      { id: "bids", label: "Bids" },
      { id: "store_settings", label: "Settings" },
    ],
  },
];

function Stat({ label, children }: { label: string; children?: ReactNode }) {
  return (
    <div className="mkmybids__stat">
      <span className="mkmybids__statlabel">{label}</span>
      <div className="mkmybids__statvalue">{children}</div>
    </div>
  );
}

function BidRow({ bid, role, archived }: { bid: Bid; role: BidRole; archived?: boolean }) {
  return (
    <div className="mkmybids__bid">
      <div className="mkmybids__bidrow">
        <div className="mkmybids__image">
          <span
            className="mkmybids__thumb"
            style={styleVars({ "--tile": bid.tile })}
            role="img"
            aria-label={bid.name}
          />
        </div>
        <div className="mkmybids__wrapper">
          <Stat label="From">
            <span className="mkmybids__profile">
              <span
                className="u-avatar"
                style={styleVars({ "--sz": "26px", "--hue": bid.hue })}
              />
              <span className="name">{bid.bidder}</span>
            </span>
          </Stat>
          <Stat label="Created">{bid.created}</Stat>
          <Stat label="Price">
            <span className="mkmybids__mana">
              <span className="mkmybids__manamark">
                <ManaMark size={13} />
              </span>
              {bid.price}
            </span>
          </Stat>
          <Stat label="Time Left">{bid.timeLeft}</Stat>

          <div className="mkmybids__actions">
            {role === "bidder" ? (
              <>
                <Button variant="primary">Update</Button>
                <Button variant="secondary">Cancel</Button>
              </>
            ) : (
              <>
                <Button variant="primary">Accept</Button>
                <Button variant="secondary">{archived ? "Unarchive" : "Archive"}</Button>
              </>
            )}
          </div>
        </div>
      </div>
      {bid.warning ? <div className="mkmybids__warning">{bid.warning}</div> : null}
    </div>
  );
}

type BidsListProps = {
  bids: Bid[];
  role: BidRole;
  archived?: boolean;
  isLoading?: boolean;
  emptyText?: string;
};

function BidsList({ bids, role, archived, isLoading, emptyText }: BidsListProps) {
  return (
    <div className="mkmybids__list">
      {bids.length === 0 && isLoading ? (
        <div className="mkmybids__center">
          <Spinner />
        </div>
      ) : null}
      {bids.length === 0 && !isLoading ? (
        <div className="mkmybids__center">
          <div className="mkmybids__empty">{emptyText}</div>
        </div>
      ) : null}
      {bids.map((bid) => (
        <BidRow key={bid.id} bid={bid} role={role} archived={archived} />
      ))}
    </div>
  );
}

type MkMyBidsProps = {
  isConnecting?: boolean;
  isLoading?: boolean;
  sellerBids?: Bid[];
  archivedBids?: Bid[];
  bidderBids?: Bid[];
  chrome?: boolean;
};

export default function MkMyBids({
  isConnecting = false,
  isLoading = false,
  sellerBids = [],
  archivedBids = [],
  bidderBids = [],
  chrome = true,
}: MkMyBidsProps) {
  const [tab, setTab] = useState<MarketplaceNavId>("my-assets");
  const [showArchived, setShowArchived] = useState(false);

  const receivedList = showArchived ? archivedBids : sellerBids;
  const toggleAmount = showArchived ? sellerBids.length : archivedBids.length;
  const showToggle = showArchived || archivedBids.length > 0;

  return (
    <MarketplaceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
      <div className="mkmybids">
        {isConnecting ? (
          <div className="mkmybids__loaderfull">
            <Spinner size={64} />
          </div>
        ) : (
          <>
            <aside className="mkmybids__rail" aria-label="Account sections">
              {RAIL.map((group) => (
                <ul className="mkmybids__menu" key={group.head}>
                  <li className="mkmybids__menuhead">{group.head}</li>
                  {group.items.map((item) => (
                    <li key={item.id}>
                      <button
                        type="button"
                        className={
                          "mkmybids__item" + (item.id === "bids" ? " is-active" : "")
                        }
                        aria-current={item.id === "bids" ? "page" : undefined}
                      >
                        {item.label}
                      </button>
                    </li>
                  ))}
                </ul>
              ))}
            </aside>

            <section className="mkmybids__main" aria-label="My bids">
              <div className="mkmybids__headermenu">
                <h3 className="mkmybids__subheader">
                  {showArchived ? "Bids archived" : "Bids received"}
                </h3>
                {showToggle ? (
                  <button
                    type="button"
                    className="mkmybids__basicbtn"
                    onClick={() => setShowArchived((v) => !v)}
                  >
                    {showArchived
                      ? `Show Received (${toggleAmount})`
                      : `Show Archived (${toggleAmount})`}
                  </button>
                ) : null}
              </div>
              <BidsList
                bids={receivedList}
                role="seller"
                archived={showArchived}
                isLoading={isLoading}
                emptyText={
                  showArchived
                    ? "You don't have any archived bids."
                    : "You haven't received any bids yet."
                }
              />

              <div className="mkmybids__headermenu">
                <h3 className="mkmybids__subheader">Bids placed</h3>
              </div>
              <BidsList
                bids={bidderBids}
                role="bidder"
                isLoading={isLoading}
                emptyText="You haven't placed any bids yet."
              />
            </section>
          </>
        )}
      </div>
    </MarketplaceChromeMaybe>
  );
}
