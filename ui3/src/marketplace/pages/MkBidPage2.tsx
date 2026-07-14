import type { ReactNode } from "react";
import { useState } from "react";
import { MarketplaceChromeMaybe, type MarketplaceNavId } from "../frames/MarketplaceChrome";
import Modal from "../../components/Modal";
import ManaMark from "../../atoms/ManaMark";
import AssetPreviewTile from "../components/AssetPreviewTile";
import "./mkbidpage2.css";

type BidItem = {
  name: string;
  collection?: string;
  rarity: string;
  network?: string;
  image?: string | null;
};

const EMPTY_ITEM: BidItem = {
  name: "",
  rarity: "",
};

type ConfirmModalProps = {
  item: BidItem;
  price: string;
  onCancel?: () => void;
  loading?: boolean;
  portal?: boolean;
};

function ConfirmModal({ item, price, onCancel, loading, portal }: ConfirmModalProps) {
  return (
    <Modal width="100%" className="modal__card--plain mkbidpage2__dialog" ariaLabel="Please confirm" portal={portal}>
        <h2 className="mkbidpage2__dialogtitle">Please confirm</h2>
        <p className="mkbidpage2__dialogtext">
          You are about to bid on <b>{item.name}</b> for{" "}
          <span className="mkbidpage2__manainline">
            <ManaMark size={13} /> {Number(price || 0).toLocaleString()}
          </span>
          .
          <br />
          Please re-enter the price to confirm:
        </p>
        <div className="mkbidpage2__field">
          <div className="mkbidpage2__manawrap">
            <span className="mkbidpage2__manaicon">
              <ManaMark size={15} />
            </span>
            <input
              className="mkbidpage2__input"
              inputMode="decimal"
              placeholder={price}
              aria-label="Confirm price"
            />
          </div>
        </div>
        <div className="mkbidpage2__dialogbtns">
          <button type="button" className="mkbidpage2__btn" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className={
              "mkbidpage2__btn mkbidpage2__btn--primary" +
              (loading ? " is-loading" : "")
            }
            aria-label={loading ? "Bid" : undefined}
          >
            {loading ? <span className="mkbidpage2__spin" aria-hidden="true" /> : "Bid"}
          </button>
        </div>
    </Modal>
  );
}

type MkBidPage2Props = {
  item?: BidItem;
  manaBalance?: string;
  submitting?: boolean;
  insufficientMana?: boolean;
  confirming?: boolean;
  lowPriceWarn?: boolean;
  chrome?: boolean;
  /** Forwarded to `Modal`. `false` renders the confirm dialog in place instead of portalling it. */
  portal?: boolean;
  banner?: ReactNode;
  onBack?: () => void;
  onCancel?: () => void;
  onBid?: (args: { price: string; expiresAt: string }) => void;
};

export default function MkBidPage2({
  item = EMPTY_ITEM,
  manaBalance,
  submitting = false,
  insufficientMana = false,
  confirming = false,
  lowPriceWarn = false,
  chrome = true,
  portal = true,
  banner,
  onBack,
  onCancel,
  onBid,
}: MkBidPage2Props) {
  const [tab, setTab] = useState<MarketplaceNavId>("collectibles");
  const [price, setPrice] = useState("");
  const [expiresAt, setExpiresAt] = useState("2026-07-20");
  const priceNum = parseFloat(String(price).replace(/,/g, ""));
  const bidBlocked = !!onBid && !(priceNum > 0);

  return (
    <MarketplaceChromeMaybe chrome={chrome} active={tab} onTab={setTab} mana={manaBalance}>
      <div className="mkbidpage2">
        <div className="mkbidpage2__page">
          {banner}
          <button
            type="button"
            className="mkbidpage2__back"
            onClick={onBack ?? onCancel}
          >
            <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
              <path
                d="M10 3 5 8l5 5"
                stroke="currentColor"
                strokeWidth="1.8"
                strokeLinecap="round"
                strokeLinejoin="round"
                fill="none"
              />
            </svg>
            <span>Back</span>
          </button>

          <div className="mkbidpage2__row">
            <div className="mkbidpage2__left">
              <AssetPreviewTile
                rarity={item.rarity}
                image={item.image}
                figure="plate"
                chipPosition="bottom"
              />
            </div>

            <div className="mkbidpage2__right">
              <div className="mkbidpage2__action">
                <h1 className="mkbidpage2__title">Place a bid</h1>
                <p className="mkbidpage2__subtitle">
                  Set a price and expiration date for your bid on{" "}
                  <b className="mkbidpage2__primary">{item.name}</b>.
                </p>

                <form
                  className="mkbidpage2__form"
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (bidBlocked || submitting || insufficientMana) return;
                    onBid?.({ price, expiresAt });
                  }}
                >
                  <div className="mkbidpage2__fields">
                    <label className="mkbidpage2__field">
                      <span className="mkbidpage2__label">Price</span>
                      <div
                        className={
                          "mkbidpage2__manawrap" +
                          (insufficientMana ? " is-error" : "")
                        }
                      >
                        <span className="mkbidpage2__manaicon">
                          <ManaMark size={16} />
                        </span>
                        <input
                          className="mkbidpage2__input"
                          inputMode="decimal"
                          value={price}
                          placeholder="1000"
                          onChange={(e) => setPrice(e.target.value)}
                          aria-label="Price"
                        />
                      </div>
                      {insufficientMana && (
                        <span className="mkbidpage2__msg is-error">
                          You don&apos;t have enough MANA
                        </span>
                      )}
                    </label>

                    <label className="mkbidpage2__field">
                      <span className="mkbidpage2__label">Expiration date</span>
                      <input
                        type="date"
                        className="mkbidpage2__input mkbidpage2__input--date"
                        value={expiresAt}
                        onChange={(e) => setExpiresAt(e.target.value)}
                        aria-label="Expiration date"
                      />
                    </label>
                  </div>

                  {lowPriceWarn ? (
                    <span className="mkbidpage2__warning">
                      MANA transactions are only gas fee free if the item is at
                      least 1 MANA. To get this item, switch your network to
                      Polygon to pay for the gas fee with MATIC.{" "}
                      <a
                        href="https://docs.decentraland.org/blockchain-integration/transactions-in-polygon"
                        className="mkbidpage2__learn"
                        target="_blank"
                        rel="noopener noreferrer"
                      >
                        <u>Learn More</u>
                      </a>
                    </span>
                  ) : (
                    <span className="mkbidpage2__freetx">
                      Pay with Polygon MANA to have gas fees{" "}
                      <span className="mkbidpage2__freecovered">
                        covered for you by the DAO
                      </span>{" "}
                      (item must be at least 1 MANA).
                    </span>
                  )}

                  <div className="mkbidpage2__buttons">
                    <button
                      type="button"
                      className="mkbidpage2__btn"
                      onClick={onCancel}
                    >
                      Cancel
                    </button>
                    <button
                      type="submit"
                      className={
                        "mkbidpage2__btn mkbidpage2__btn--primary" +
                        (submitting || insufficientMana || bidBlocked
                          ? " is-disabled"
                          : "") +
                        (submitting ? " is-loading" : "")
                      }
                      disabled={submitting || insufficientMana || bidBlocked}
                      aria-label={submitting ? "Bid" : undefined}
                    >
                      {submitting ? (
                        <span className="mkbidpage2__spin" aria-hidden="true" />
                      ) : (
                        "Bid"
                      )}
                    </button>
                  </div>
                </form>
              </div>
            </div>
          </div>
        </div>

        {confirming && (
          <ConfirmModal item={item} price={price} loading={false} portal={portal} />
        )}
      </div>
    </MarketplaceChromeMaybe>
  );
}
