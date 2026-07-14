import type { ReactNode } from "react";
import { useState } from "react";
import { MarketplaceChromeMaybe, type MarketplaceNavId } from "../frames/MarketplaceChrome";
import Button from "../../atoms/Button";
import Modal from "../../components/Modal";
import ManaMark from "../../atoms/ManaMark";
import AssetPreviewTile from "../components/AssetPreviewTile";
import "./mkbidpage.css";

type Nft = {
  name: string;
  category: string;
  rarity: string;
  network: string;
};

type BidStage = "form" | "authorize" | "confirm";

const COPY = {
  title: "Place a bid",
  subtitle_lead: "Set a price and expiration date for your bid on",
  price: "Price",
  expiration_date: "Expiration date",
  invalid_date: "This date has already passed",
  not_enough_mana: "You don't have enough MANA",
  submit: "Bid",
  cancel: "Cancel",
  proceed: "Proceed",
  fee_covered_a: "Pay with Polygon MANA to have gas fees ",
  fee_covered_b: "covered for you by the DAO",
  fee_covered_c: " (item must be at least 1 MANA).",
  price_too_low_a:
    "MANA transactions are only gas fee free if the item is at least 1 MANA. To get this item, switch your network to Polygon to pay for the gas fee with MATIC. ",
  learn_more: "Learn More",
  confirm_title: "Please confirm",
  confirm_line_one_a: "You are about to bid on ",
  confirm_line_one_b: ".",
  confirm_line_two: "Please re-enter the price to confirm:",
  auth_title: "Authorize the operation",
  auth_action: "make an offer for",
  authorize: "Authorize",
  error: "Error",
};

const EMPTY_NFT: Nft = {
  name: "",
  category: "",
  rarity: "",
  network: "",
};

function toNum(n: string | number) {
  return parseFloat(String(n).replace(/,/g, ""));
}
function fmtMana(n: string | number) {
  const v = toNum(n);
  return Number.isFinite(v) ? v.toLocaleString() : "0";
}

type BidModalProps = {
  nft: Nft;
  price: string;
  error?: string;
  loading?: boolean;
  onCancel?: () => void;
  onConfirm?: () => void;
};

function ConfirmModal({ nft, price, error, loading, onCancel, onConfirm }: BidModalProps) {
  const [confirmed, setConfirmed] = useState("");
  const target = parseFloat(String(price).replace(/,/g, "") || "0").toString();
  const isDisabled = loading || target !== confirmed;
  return (
    <Modal width={460} onClose={loading ? undefined : onCancel} className="mkbidpage__modalcard" ariaLabel={COPY.confirm_title}>
      <div className="mkbidpage__confirm">
        <div className="mkbidpage__confirmhead">
          <h2 className="mkbidpage__confirmtitle">{COPY.confirm_title}</h2>
          <button
            type="button"
            className="mkbidpage__close"
            aria-label="Close"
            onClick={loading ? undefined : onCancel}
          >
            <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <path d="M5 5l10 10M15 5L5 15" />
            </svg>
          </button>
        </div>

        <div className="mkbidpage__confirmbody">
          {COPY.confirm_line_one_a}
          <b>{nft.name}</b> for{" "}
          <span className="mkbidpage__amount">
            <ManaMark size={14} />
            {fmtMana(price)}
          </span>
          {COPY.confirm_line_one_b}
          <p className="mkbidpage__confirmline2">{COPY.confirm_line_two}</p>
        </div>

        <label className="mkbidpage__confirmlabel" htmlFor="mkbid-confirm">
          {COPY.price}
        </label>
        <div className="mkbidpage__inputbox">
          <span className="mkbidpage__mana"><ManaMark /></span>
          <input
            id="mkbid-confirm"
            className="mkbidpage__input"
            type="text"
            inputMode="decimal"
            placeholder={target}
            value={confirmed}
            disabled={loading}
            onChange={(e) => setConfirmed(e.target.value)}
          />
        </div>

        {error ? (
          <p className="mkbidpage__errmsg">
            <strong>{COPY.error}</strong>
            {error}
          </p>
        ) : null}

        <div className="mkbidpage__confirmactions">
          <Button variant="secondary" className="mkbidpage__cancel" disabled={loading} onClick={onCancel}>
            {COPY.cancel}
          </Button>
          <Button variant="primary" disabled={isDisabled} onClick={onConfirm}>
            {loading ? <span className="mkbidpage__spin" aria-hidden="true" /> : null}
            {COPY.proceed}
          </Button>
        </div>
      </div>
    </Modal>
  );
}

function AuthorizationModal({ nft, price, error, loading, onCancel, onConfirm }: BidModalProps) {
  return (
    <Modal width={480} onClose={loading ? undefined : onCancel} className="mkbidpage__modalcard" ariaLabel={COPY.auth_title}>
      <div className="mkbidpage__confirm">
        <div className="mkbidpage__confirmhead">
          <h2 className="mkbidpage__confirmtitle">{COPY.auth_title}</h2>
          <button
            type="button"
            className="mkbidpage__close"
            aria-label="Close"
            onClick={loading ? undefined : onCancel}
          >
            <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <path d="M5 5l10 10M15 5L5 15" />
            </svg>
          </button>
        </div>

        <p className="mkbidpage__authlead">
          To {COPY.auth_action} <b>{nft.name}</b> you need to allow the marketplace contract to operate
          MANA on your behalf.
        </p>

        <ol className="mkbidpage__steps">
          <li className="mkbidpage__step is-current">
            <span className="mkbidpage__stepdot" />
            <div>
              <div className="mkbidpage__steptitle">Authorize MANA</div>
              <div className="mkbidpage__stepsub">
                Spend up to{" "}
                <span className="mkbidpage__amount">
                  <ManaMark size={13} />
                  {fmtMana(price)}
                </span>
              </div>
            </div>
          </li>
          <li className="mkbidpage__step">
            <span className="mkbidpage__stepdot" />
            <div>
              <div className="mkbidpage__steptitle">Confirm transaction</div>
              <div className="mkbidpage__stepsub">Place your bid</div>
            </div>
          </li>
        </ol>

        {error ? (
          <p className="mkbidpage__errmsg">
            <strong>{COPY.error}</strong>
            {error}
          </p>
        ) : null}

        <div className="mkbidpage__confirmactions">
          <Button variant="secondary" className="mkbidpage__cancel" disabled={loading} onClick={onCancel}>
            {COPY.cancel}
          </Button>
          <Button variant="primary" disabled={loading} onClick={onConfirm}>
            {loading ? <span className="mkbidpage__spin" aria-hidden="true" /> : null}
            {COPY.authorize}
          </Button>
        </div>
      </div>
    </Modal>
  );
}

type MkBidPageProps = {
  nft?: Nft;
  manaBalance?: number;
  notOnPolygon?: boolean;
  initialPrice?: string;
  initialExpiration?: string;
  needsAuthorization?: boolean;
  isPlacingBid?: boolean;
  bidError?: string;
  initialStage?: BidStage;
  requiresSignIn?: boolean;
  onSignIn?: () => void;
  chrome?: boolean;
  banner?: ReactNode;
  onBack?: () => void;
  onCancel?: () => void;
  onSubmit?: (args: { price: string; expiration: string }) => void;
};

export default function MkBidPage({
  nft = EMPTY_NFT,
  manaBalance = 0,
  notOnPolygon = false,
  initialPrice = "",
  initialExpiration = "2026-07-20",
  needsAuthorization = false,
  isPlacingBid = false,
  bidError = "",
  initialStage = "form",
  requiresSignIn = false,
  onSignIn,
  chrome = true,
  banner,
  onBack,
  onCancel,
  onSubmit,
}: MkBidPageProps) {
  const [tab, setTab] = useState<MarketplaceNavId>("collectibles");
  const [price, setPrice] = useState(initialPrice);
  const [expiration, setExpiration] = useState(initialExpiration);
  const [stage, setStage] = useState(initialStage);

  const priceNum = toNum(price);
  const isInvalidPrice = price !== "" && !(priceNum > 0);
  const isInvalidDate = !!expiration && new Date(`${expiration} 00:00:00`).getTime() < Date.now();
  const hasInsufficientMANA = price !== "" && priceNum > 0 && priceNum > manaBalance;
  const hasLowPriceForMetaTx = notOnPolygon && priceNum > 0 && priceNum < 1;

  const isDisabled =
    !(priceNum > 0) ||
    isInvalidDate ||
    hasInsufficientMANA ||
    hasLowPriceForMetaTx ||
    isPlacingBid;

  return (
    <MarketplaceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
      <div className="mkbidpage">
        <div className="mkbidpage__page">
          {banner}
          <button
            type="button"
            className="mkbidpage__back"
            onClick={onBack ?? onCancel}
          >
            <svg viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M12 4l-6 6 6 6" />
            </svg>
            Back
          </button>

          <div className="mkbidpage__row">
            <div className="mkbidpage__left">
              <AssetPreviewTile rarity={nft.rarity} />
            </div>

            <div className="mkbidpage__right">
              <div className="mkbidpage__bidaction">
                <h1 className="mkbidpage__title">{COPY.title}</h1>
                <p className="mkbidpage__subtitle">
                  {COPY.subtitle_lead} <b>{nft.name}</b>.
                </p>

                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (requiresSignIn) {
                      onSignIn?.();
                      return;
                    }
                    if (isDisabled) return;
                    if (onSubmit) {
                      onSubmit({ price, expiration });
                      return;
                    }
                    setStage(needsAuthorization ? "authorize" : "confirm");
                  }}
                >
                  <div className="mkbidpage__fields">
                    <div className="mkbidpage__field">
                      <label className="mkbidpage__label" htmlFor="mkbid-price">
                        {COPY.price}
                      </label>
                      <div className={"mkbidpage__inputbox" + ((isInvalidPrice || hasInsufficientMANA) ? " is-error" : "")}>
                        <span className="mkbidpage__mana"><ManaMark /></span>
                        <input
                          id="mkbid-price"
                          className="mkbidpage__input"
                          type="text"
                          inputMode="decimal"
                          placeholder="1000"
                          autoFocus
                          value={price}
                          onChange={(e) => setPrice(e.target.value)}
                        />
                      </div>
                      {hasInsufficientMANA ? (
                        <p className="mkbidpage__msg">{COPY.not_enough_mana}</p>
                      ) : null}
                    </div>

                    <div className="mkbidpage__field">
                      <label className="mkbidpage__label" htmlFor="mkbid-exp">
                        {COPY.expiration_date}
                      </label>
                      <div className={"mkbidpage__inputbox" + (isInvalidDate ? " is-error" : "")}>
                        <input
                          id="mkbid-exp"
                          className="mkbidpage__input"
                          type="date"
                          value={expiration}
                          onChange={(e) => setExpiration(e.target.value)}
                        />
                      </div>
                      {isInvalidDate ? (
                        <p className="mkbidpage__msg">{COPY.invalid_date}</p>
                      ) : null}
                    </div>
                  </div>

                  {hasLowPriceForMetaTx ? (
                    <span className="mkbidpage__warning">
                      {COPY.price_too_low_a}
                      <a
                        className="mkbidpage__learnmore"
                        href="https://docs.decentraland.org/blockchain-integration/transactions-in-polygon"
                        target="_blank"
                        rel="noreferrer"
                      >
                        <u>{COPY.learn_more}</u>
                      </a>
                    </span>
                  ) : (
                    <span className="mkbidpage__rememberfree">
                      {COPY.fee_covered_a}
                      <span className="mkbidpage__feecovered">{COPY.fee_covered_b}</span>
                      {COPY.fee_covered_c}
                    </span>
                  )}

                  <div className="mkbidpage__buttons">
                    <Button
                      type="button"
                      variant="secondary"
                      className="mkbidpage__cancel"
                      onClick={onCancel}
                    >
                      {COPY.cancel}
                    </Button>
                    <Button type="submit" variant="primary" disabled={requiresSignIn ? false : isDisabled}>
                      {isPlacingBid ? <span className="mkbidpage__spin" aria-hidden="true" /> : null}
                      {COPY.submit}
                    </Button>
                  </div>
                </form>
              </div>
            </div>
          </div>
        </div>

        {stage === "authorize" ? (
          <AuthorizationModal
            nft={nft}
            price={price}
            error={bidError}
            loading={isPlacingBid}
            onCancel={() => setStage("form")}
            onConfirm={() => setStage("confirm")}
          />
        ) : null}

        {stage === "confirm" ? (
          <ConfirmModal
            nft={nft}
            price={price}
            error={bidError}
            loading={isPlacingBid}
            onCancel={() => setStage("form")}
            onConfirm={() => setStage("form")}
          />
        ) : null}
      </div>
    </MarketplaceChromeMaybe>
  );
}
