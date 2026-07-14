import { useState } from "react";
import { MarketplaceChromeMaybe, type MarketplaceNavId } from "../frames/MarketplaceChrome";
import Button from "../../atoms/Button";
import ManaMark from "../../atoms/ManaMark";
import AssetPreviewTile from "../components/AssetPreviewTile";
import "./mkcancelsalepage.css";

const COPY = {
  title: "Remove listing",
  subtitle_lead: "You are about to remove the listing of",
  subtitle_for: "for",
  not_for_sale_lead: "There are no active listings for",
  invalid_owner_lead: "You are not the owner of",
  cancel: "Cancel",
  submit: "Confirm",
  pending_title: "Removing your listing",
  pending_body: "Confirm the transaction in your wallet and wait for it to be mined.",
  success_title: "Listing removed",
  success_body: "Your listing has been removed. The item is no longer for sale.",
  success_view: "View item",
  auth_title: "Authorize MANA",
  auth_body:
    "In order to continue you will need to authorize the Marketplace contract to operate MANA tokens on your behalf. This only has to be done once.",
  auth_submit: "Authorize",
};

function fmtMana(n: string | number | null | undefined) {
  const v = parseFloat(String(n).replace(/,/g, ""));
  return Number.isFinite(v) ? v.toLocaleString() : "0";
}

type CancelNft = {
  name: string;
  category?: string;
  rarity: string;
  network?: string;
};

type CancelOrder = {
  price: string;
  owner?: string;
};

const EMPTY_NFT: CancelNft = {
  name: "",
  rarity: "",
};

type MkCancelSalePageProps = {
  nft?: CancelNft;
  order?: CancelOrder | null;
  ownership?: "self" | "other" | "none";
  status?: "confirmation" | "authorize" | "pending" | "success";
  onConfirm?: () => void;
  onCancel?: () => void;
  chrome?: boolean;
};

export default function MkCancelSalePage({
  nft = EMPTY_NFT,
  order = null,
  ownership = "self",
  status = "confirmation",
  onConfirm,
  onCancel,
  chrome = true,
}: MkCancelSalePageProps) {
  const [tab, setTab] = useState<MarketplaceNavId>("my-assets");

  const isNotForSale = !order || ownership === "none";
  const isInvalidOwner = !isNotForSale && ownership === "other";
  const isDisabled = isNotForSale || isInvalidOwner;
  const isLoading = status === "pending";

  if (status === "pending") {
    return (
      <MarketplaceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
        <div className="mkcancelsalepage">
          <div className="mkcancelsalepage__page mkcancelsalepage__centered">
            <div className="mkcancelsalepage__statuscard">
              <div className="mkcancelsalepage__spinner" aria-hidden="true" />
              <h1 className="mkcancelsalepage__statustitle">{COPY.pending_title}</h1>
              <p className="mkcancelsalepage__statusbody">{COPY.pending_body}</p>
            </div>
          </div>
        </div>
      </MarketplaceChromeMaybe>
    );
  }

  if (status === "success") {
    return (
      <MarketplaceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
        <div className="mkcancelsalepage">
          <div className="mkcancelsalepage__page mkcancelsalepage__centered">
            <div className="mkcancelsalepage__statuscard">
              <div className="mkcancelsalepage__check" aria-hidden="true">
                <svg viewBox="0 0 24 24" width="34" height="34" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M20 6 9 17l-5-5" />
                </svg>
              </div>
              <h1 className="mkcancelsalepage__statustitle">{COPY.success_title}</h1>
              <p className="mkcancelsalepage__statusbody">{COPY.success_body}</p>
              <Button variant="primary">{COPY.success_view}</Button>
            </div>
          </div>
        </div>
      </MarketplaceChromeMaybe>
    );
  }

  const isAuthorize = status === "authorize";

  return (
    <MarketplaceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
      <div className="mkcancelsalepage">
        <div className="mkcancelsalepage__page">
          <button type="button" className="mkcancelsalepage__back" onClick={onCancel}>
            <svg viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M12 4l-6 6 6 6" />
            </svg>
            Back
          </button>

          <div className="mkcancelsalepage__row">
            <div className="mkcancelsalepage__left">
              <AssetPreviewTile rarity={nft.rarity} />
            </div>

            <div className="mkcancelsalepage__right">
              {isAuthorize ? (
                <>
                  <h1 className="mkcancelsalepage__title">{COPY.auth_title}</h1>
                  <div className="mkcancelsalepage__subtitle">{COPY.auth_body}</div>
                  <div className="mkcancelsalepage__buttons">
                    <Button variant="secondary" className="mkcancelsalepage__cancel" onClick={onCancel}>
                      {COPY.cancel}
                    </Button>
                    <Button variant="primary" onClick={onConfirm}>
                      {COPY.auth_submit}
                    </Button>
                  </div>
                </>
              ) : (
                <>
                  <h1 className="mkcancelsalepage__title">{COPY.title}</h1>

                  <div className="mkcancelsalepage__subtitle">
                    {isNotForSale ? (
                      <>
                        {COPY.not_for_sale_lead} <b>{nft.name}</b>.
                      </>
                    ) : isInvalidOwner ? (
                      <>
                        {COPY.invalid_owner_lead} <b>{nft.name}</b>.
                      </>
                    ) : (
                      <>
                        {COPY.subtitle_lead} <b>{nft.name}</b> {COPY.subtitle_for}{" "}
                        <span className="mkcancelsalepage__amount">
                          <ManaMark size={14} />
                          {fmtMana(order?.price)}
                        </span>
                        .
                      </>
                    )}
                  </div>

                  <div className="mkcancelsalepage__buttons">
                    <Button type="button" variant="secondary" className="mkcancelsalepage__cancel" disabled={isLoading} onClick={onCancel}>
                      {COPY.cancel}
                    </Button>
                    <Button type="button" variant="primary" disabled={isDisabled || isLoading} onClick={onConfirm}>
                      {COPY.submit}
                    </Button>
                  </div>
                </>
              )}
            </div>
          </div>
        </div>
      </div>
    </MarketplaceChromeMaybe>
  );
}
