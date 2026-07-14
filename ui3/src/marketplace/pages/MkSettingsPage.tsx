import type { ReactNode } from "react";
import { useState } from "react";
import { MarketplaceChromeMaybe, type MarketplaceNavId } from "../frames/MarketplaceChrome";
import Spinner from "../../atoms/Spinner";
import "./mksettingspage.css";

type AuthRow = { id: string; address: string; contract: string; network: string };
type SellRow = { id: string; contract: string; token: string; network: string };

const MARKETPLACE_ETH = "0x8e5660b4ab70168b5a6feea0e0315cb49c8cd539";
const MARKETPLACE_MATIC = "0x480a0f4e360e8964e68858dd231c2922f1df45ef";
const OFFCHAIN_ETH = "0x540fb08edb56aae562864b390542c97f562825ba";
const OFFCHAIN_MATIC = "0x02ee9faf31edf0a48d4f0e9c0b8e8f3f8c3f1d1";
const COLLECTION_STORE = "0x214ffc0f0103735728dc66b61a22e4f163e275bb";
const CREDITS_MANAGER = "0xe9f961e6ea3f02b3f59b8c0c25f1f6d6e4f9e9c1";
const BIDS_ETH = "0xe479dfd9664c693b2e2992300930b00bfde08233";
const BIDS_MATIC = "0xb96697fa4a3361ba35b774a42c58daccaad1b8e1";
const RENTALS = "0x3a1469499d0be105d4f77045ca403a5f6dc2f3f5";

const AUTH: { buying: AuthRow[]; bidding: AuthRow[]; renting: AuthRow[] } = {
  buying: [
    { id: "mp-eth", address: MARKETPLACE_ETH, contract: "Marketplace", network: "Ethereum" },
    { id: "mp-matic", address: MARKETPLACE_MATIC, contract: "Marketplace", network: "Polygon" },
    { id: "off-eth", address: OFFCHAIN_ETH, contract: "OffChainMarketplace", network: "Ethereum" },
    { id: "off-matic", address: OFFCHAIN_MATIC, contract: "OffChainMarketplace", network: "Polygon" },
    { id: "store", address: COLLECTION_STORE, contract: "CollectionStore", network: "Polygon" },
    { id: "credits", address: CREDITS_MANAGER, contract: "CreditsManager", network: "Polygon" },
  ],
  bidding: [
    { id: "bids-eth", address: BIDS_ETH, contract: "Bids", network: "Ethereum" },
    { id: "bids-matic", address: BIDS_MATIC, contract: "Bids", network: "Polygon" },
  ],
  renting: [
    { id: "rentals", address: RENTALS, contract: "Rentals", network: "Ethereum" },
  ],
};

function Authorization({
  token = "MANA",
  contract,
  address,
  network,
  checked,
  pending,
  onChange,
}: {
  token?: string;
  contract: string;
  address?: string;
  network: string;
  checked?: boolean;
  pending?: boolean;
  onChange?: (next: boolean) => void;
}) {
  return (
    <div className="mks__auth">
      <div className={"mks__field" + (pending ? " is-pending" : "")}>
        {pending ? (
          <a
            href="/marketplace/activity"
            className="mks__loadertip"
            title="Transaction pending, click the spinner to go to your activity for a detailed status"
          >
            <Spinner size={20} />
          </a>
        ) : (
          <button
            type="button"
            role="radio"
            aria-checked={!!checked}
            className={"mks__radio" + (checked ? " is-checked" : "")}
            onClick={() => onChange?.(!checked)}
          >
            <span className="mks__radiomark" aria-hidden="true" />
            <span className="mks__radiolabel">{token}</span>
          </button>
        )}
        <div className="mks__radiodesc mks__secondary">
          Authorize the{" "}
          <a
            href={address ? `${network === "Ethereum" ? "https://etherscan.io" : "https://polygonscan.com"}/address/${address}` : undefined}
            className="mks__contractlink"
            target="_blank"
            rel="noopener noreferrer"
            title={address}
          >
            {contract}
          </a>{" "}
          contract to operate {token} on your behalf on {network}
        </div>
      </div>
    </div>
  );
}

function Blockie({ seed }: { seed: string }) {
  const cells: { x: number; y: number; on: boolean }[] = [];
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  const rand = () => {
    h ^= h << 13; h ^= h >>> 17; h ^= h << 5; h >>>= 0;
    return h / 4294967296;
  };
  const hue = Math.floor(rand() * 360);
  const fg = `hsl(${hue} 62% 56%)`;
  const bg = `hsl(${hue} 28% 22%)`;
  const N = 8;
  for (let y = 0; y < N; y++) {
    for (let x = 0; x < N / 2 + 1; x++) {
      const on = rand() > 0.5;
      cells.push({ x, y, on });
      if (x < N / 2) cells.push({ x: N - 1 - x, y, on });
    }
  }
  return (
    <span className="mks__blockie" style={{ background: bg }} aria-hidden="true">
      {cells.map((c, i) =>
        c.on ? (
          <span
            key={i}
            className="mks__blockiecell"
            style={{ left: `${(c.x / N) * 100}%`, top: `${(c.y / N) * 100}%`, background: fg }}
          />
        ) : null,
      )}
    </span>
  );
}

export default function MkSettingsPage({
  address = "",
  isLoading = false,
  hasError = false,
  selling = [],
  chrome = true,
}: {
  address?: string;
  isLoading?: boolean;
  hasError?: boolean;
  selling?: SellRow[];
  chrome?: boolean;
}) {
  const [tab, setTab] = useState<MarketplaceNavId | null>(null);
  const [copied, setCopied] = useState(false);

  const [granted, setGranted] = useState<Record<string, boolean>>({
    "mp-eth": true,
    "mp-matic": true,
    "off-matic": true,
    store: true,
    "bids-matic": true,
    rentals: true,
    "sell-mp-matic": true,
  });
  const pending: Record<string, boolean> = { "off-eth": true };

  const toggle = (id: string, next: boolean) => setGranted((g) => ({ ...g, [id]: next }));

  const copy = () => {
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };

  const renderGroup = (label: ReactNode, rows: AuthRow[], extra: SellRow[] = []) => (
    <div className="mks__checks">
      <label className="mks__grouplabel mks__secondary">{label}</label>
      {rows.map((r) => (
        <Authorization
          key={r.id}
          contract={r.contract}
          address={r.address}
          network={r.network}
          checked={!!granted[r.id]}
          pending={!!pending[r.id]}
          onChange={(next) => toggle(r.id, next)}
        />
      ))}
      {extra.map((r) => (
        <Authorization
          key={r.id}
          token={r.token}
          contract={r.contract}
          network={r.network}
          checked={!!granted[r.id]}
          pending={!!pending[r.id]}
          onChange={(next) => toggle(r.id, next)}
        />
      ))}
    </div>
  );

  return (
    <MarketplaceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
      <div className="mks">
        <div className="mks__page">
          <div className="mks__row">
            <div className="mks__leftcol mks__secondary">Address</div>
            <div className="mks__rightcol">
              <div className="mks__blockiebox">
                <Blockie seed={address} />
              </div>
              <div className="mks__addrbox">
                <div className="mks__addr">{address}</div>
                <div role="button" aria-label="copy" className="mks__copy" onClick={copy}>
                  {copied ? (
                    <span className="mks__copytext">COPIED &#x2714;</span>
                  ) : (
                    <span className="mks__copytext mks__link">COPY ADDRESS</span>
                  )}
                </div>
              </div>
            </div>
          </div>

          <div className="mks__row">
            <div className="mks__leftcol mks__secondary">Authorizations</div>
            <div className="mks__rightcol">
              {isLoading ? (
                <div className="mks__loaderbox">
                  <Spinner size={64} />
                </div>
              ) : (
                <div className="mks__checkscontainer">
                  <p className="mks__simnote" role="note">
                    <strong>Simulated</strong> &#x2014; authorization management isn&apos;t
                    wired on-chain on this realm yet. The states shown are a
                    preview, and toggling here doesn&apos;t read or change your
                    wallet&apos;s real contract approvals.
                  </p>
                  {hasError ? (
                    <div className="mks__checks">
                      <p className="mks__danger">
                        We couldn't load your authorizations, please try again
                        <br />
                        If the error persist don't hesitate to contact us
                      </p>
                    </div>
                  ) : (
                    <form className="mks__form">
                      {renderGroup("For buying", AUTH.buying)}
                      {renderGroup("For bidding", AUTH.bidding)}
                      {renderGroup("For renting", AUTH.renting)}
                      {selling.length > 0 ? renderGroup("For selling", [], selling) : null}
                    </form>
                  )}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </MarketplaceChromeMaybe>
  );
}
