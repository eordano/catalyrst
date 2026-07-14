import type { ChangeEvent } from "react";
import { useState } from "react";
import { MarketplaceChromeMaybe } from "../frames/MarketplaceChrome";
import AssetPreviewTile from "../components/AssetPreviewTile";
import "./mktransferpage.css";
import { ChevronLeft } from "../../atoms/icons";
import Button from "../../atoms/Button";

type Nft = {
  contractAddress: string;
  tokenId: string;
  name: string;
  category: string;
  rarity: string;
  network: string;
};

type TransferStatus =
  | "form"
  | "transferring"
  | "for_sale"
  | "invalid_owner"
  | "success";

const COPY = {
  title: (category: string) => `Transfer ${category}`,
  subtitle: "You are about to transfer",
  forSale: "can't be transferred because it's on sale.",
  invalidOwner: "You are not the owner of",
  invalidAddress: "That's not a valid address",
  recipient: "Recepient address",
  warning1: "Remember that transferring is an irreversible operation.",
  warning2: "Please check the address carefully.",
  submit: "Transfer",
  cancel: "Cancel",
};

const CATEGORY_LABEL: Record<string, string> = {
  wearable: "Wearable",
  emote: "Emote",
  parcel: "Parcel",
  estate: "Estate",
  ens: "Name",
};

const EMPTY_NFT: Nft = {
  contractAddress: "",
  tokenId: "",
  name: "",
  category: "",
  rarity: "",
  network: "",
};

const shorten = (addr: string) =>
  addr && addr.length > 12 ? `${addr.slice(0, 6)}\u{2026}${addr.slice(-4)}` : addr;

const looksLikeAddress = (v: string) => /^0x[a-fA-F0-9]{40}$/.test(v.trim());

type BackButtonProps = {
  onClick?: () => void;
};

function BackButton({ onClick }: BackButtonProps) {
  return (
    <button type="button" className="mktransferpage__back" aria-label="Back" onClick={onClick}>
      <ChevronLeft size={9} />
    </button>
  );
}

type MkTransferPageProps = {
  nft?: Nft;
  status?: TransferStatus;
  txHash?: string;
  chrome?: boolean;
};

export default function MkTransferPage({
  nft = EMPTY_NFT,
  status = "form",
  txHash = "",
  chrome = true,
}: MkTransferPageProps) {
  const [address, setAddress] = useState("");
  const [resolved, setResolved] = useState("");
  const [touched, setTouched] = useState(false);

  const isTransferring = status === "transferring";
  const isError = status === "for_sale" || status === "invalid_owner";
  const isSuccess = status === "success";
  const category = CATEGORY_LABEL[nft.category] || nft.category;

  const invalid = touched && !!address && !looksLikeAddress(address) && !resolved;
  const canTransfer = !isError;
  const disabled = !address || invalid || isTransferring || isError;

  const handleChange = (e: ChangeEvent<HTMLInputElement>) => {
    const v = e.target.value;
    setAddress(v);
    setTouched(true);
    setResolved(/\.eth$/.test(v.trim()) ? "0x09f1c2a4b6d8e0f2a4c6b8d0e2f4a6c8b0d2e4f6" : "");
  };

  return (
    <MarketplaceChromeMaybe chrome={chrome} active="my-assets">
      <div className="mktransferpage">
        <div className="mktransferpage__action">
          <BackButton />

          {isSuccess ? (
            <div className="mktransferpage__success">
              <div className="mktransferpage__successicon" aria-hidden="true">
                <svg viewBox="0 0 24 24" width="34" height="34" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M20 6 9 17l-5-5" />
                </svg>
              </div>
              <h1 className="mktransferpage__title">Transfer submitted</h1>
              <p className="mktransferpage__subtitle">
                {COPY.subtitle} <b>{nft.name}</b>.
              </p>
              {txHash ? (
                <a
                  className="mktransferpage__txlink"
                  href={`https://polygonscan.com/tx/${txHash}`}
                  target="_blank"
                  rel="noreferrer"
                >
                  {shorten(txHash)}
                  <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                    <path d="M6 3h7v7M13 3 6.5 9.5M11 9v4H3V5h4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" fill="none" />
                  </svg>
                </a>
              ) : null}
            </div>
          ) : (
            <div className="mktransferpage__row">
              <div className="mktransferpage__left">
                <AssetPreviewTile rarity={nft.rarity} figure="inset" label={null} />
              </div>

              <div className="mktransferpage__right">
                <h1 className="mktransferpage__title">{COPY.title(category)}</h1>

                <div className={"mktransferpage__subtitle" + (isError ? " is-error" : "")}>
                  {status === "for_sale" ? (
                    <>
                      <b>{nft.name}</b> {COPY.forSale}
                    </>
                  ) : status === "invalid_owner" ? (
                    <>
                      {COPY.invalidOwner} <b>{nft.name}</b>.
                    </>
                  ) : (
                    <>
                      {COPY.subtitle} <b>{nft.name}</b>.
                    </>
                  )}
                </div>

                <form
                  className="mktransferpage__form"
                  onSubmit={(e) => e.preventDefault()}
                >
                  <div className="mktransferpage__fields">
                    <div className="mktransferpage__field">
                      <label className="mktransferpage__label" htmlFor="mktransferpage-addr">
                        {COPY.recipient}
                      </label>
                      <div className="mktransferpage__inputwrap">
                        <input
                          id="mktransferpage-addr"
                          type="text"
                          className={"mktransferpage__input" + (invalid ? " is-error" : "")}
                          placeholder="0x..."
                          value={address}
                          disabled={!canTransfer}
                          onChange={handleChange}
                        />
                        {resolved ? (
                          <span className="mktransferpage__resolved">{shorten(resolved)}</span>
                        ) : null}
                      </div>
                      {invalid ? (
                        <div className="mktransferpage__error">{COPY.invalidAddress}</div>
                      ) : null}
                    </div>
                  </div>

                  {canTransfer ? (
                    <div className="mktransferpage__warning">
                      {COPY.warning1}
                      <br />
                      {COPY.warning2}
                    </div>
                  ) : null}

                  <div className="mktransferpage__buttons">
                    <Button type="button" variant="secondary" disabled={isTransferring}>
                      {COPY.cancel}
                    </Button>
                    <Button type="submit" variant="primary" disabled={disabled}>
                      {isTransferring ? (
                        <span className="mktransferpage__spinner" aria-hidden="true" />
                      ) : null}
                      {COPY.submit}
                    </Button>
                  </div>
                </form>
              </div>
            </div>
          )}
        </div>
      </div>
    </MarketplaceChromeMaybe>
  );
}
