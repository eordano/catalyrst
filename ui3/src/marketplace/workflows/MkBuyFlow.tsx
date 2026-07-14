import type { CSSProperties, ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import ManaMark from "../../atoms/ManaMark";
import Modal from "../../components/Modal";
import { suffixLabel, type LabelSuffixProps } from "../../components/labelSuffix";
import "./mkbuyflow.css";
import { ChevronLeft } from "../../atoms/icons";

export type ChainOption = { id: string; name: string; hue: number };
export type TokenOption = { symbol: string; balance?: string; mana?: boolean; hue?: number };

const DEFAULT_CHAINS: ChainOption[] = [
  { id: "polygon", name: "Polygon", hue: 268 },
  { id: "ethereum", name: "Ethereum", hue: 210 },
];
const defaultTokens = (manaBalance: string): TokenOption[] => [
  { symbol: "MANA", mana: true, balance: manaBalance },
  { symbol: "USDC", hue: 210, balance: "0.00" },
  { symbol: "ETH", hue: 210, balance: "0.00" },
  { symbol: "DAI", hue: 45, balance: "0.00" },
];

type TokenIconProps = {
  size?: number;
  hue?: number;
  mana?: boolean;
};

const TokenIcon = ({ size = 24, hue = 254, mana = false }: TokenIconProps) => {
  if (mana) {
    return (
      <span className="mkbuyflow__tokenicon mkbuyflow__tokenicon--mana" style={{ width: size, height: size }}>
        <ManaMark size={Math.round(size * 0.62)} />
      </span>
    );
  }
  const style: CSSProperties & { "--hue": number } = { width: size, height: size, "--hue": hue };
  return (
    <span
      className="mkbuyflow__tokenicon"
      style={style}
      aria-hidden="true"
    />
  );
};

const ChevronDown = () => (
  <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true" className="mkbuyflow__chev">
    <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" fill="none" />
  </svg>
);

type SelectorMenuProps = {
  className?: string;
  ariaLabel: string;
  disabled?: boolean;
  hideChevronWhenDisabled?: boolean;
  current: string;
  trigger: ReactNode;
  options: { id: string; content: ReactNode }[];
  onSelect: (id: string) => void;
};

function SelectorMenu({
  className,
  ariaLabel,
  disabled,
  hideChevronWhenDisabled,
  current,
  trigger,
  options,
  onSelect,
}: SelectorMenuProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    function onDoc(e: MouseEvent) {
      if (ref.current && e.target instanceof Node && !ref.current.contains(e.target)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const showChevron = !(disabled && hideChevronWhenDisabled);
  return (
    <div className="mkbuyflow__selwrap" ref={ref}>
      <button
        type="button"
        className={className}
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        {trigger}
        {showChevron ? <ChevronDown /> : null}
      </button>
      {open && !disabled ? (
        <ul className="mkbuyflow__menu" role="listbox" aria-label={ariaLabel}>
          {options.map((o) => (
            <li
              key={o.id}
              role="option"
              aria-selected={o.id === current}
              tabIndex={0}
              className={"mkbuyflow__menuitem" + (o.id === current ? " is-sel" : "")}
              onClick={() => {
                onSelect(o.id);
                setOpen(false);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onSelect(o.id);
                  setOpen(false);
                }
              }}
            >
              {o.content}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

const InfoMark = () => (
  <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
    <circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" strokeWidth="1.3" />
    <circle cx="8" cy="4.6" r="0.95" fill="currentColor" />
    <path d="M8 7v5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
  </svg>
);

const ClockMark = () => (
  <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
    <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" strokeWidth="1.3" />
    <path d="M8 4.5V8l2.5 1.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" fill="none" />
  </svg>
);

type BuyAsset = {
  name: string;
  rarity: string;
  network: string;
  kind: string;
  priceMana: string;
  priceUsd?: string;
  image?: string | null;
};

type BuyState =
  | "default"
  | "card"
  | "buying"
  | "loadingRoute"
  | "priceTooLow"
  | "insufficient"
  | "routeUnavailable";

const EMPTY_ASSET: BuyAsset = {
  name: "",
  rarity: "",
  network: "",
  kind: "",
  priceMana: "",
};

const ASSET_DESCRIPTION: Record<string, string> = {
  ens: "Decentraland NAMEs",
  emote: "Decentraland Emotes",
  wearable: "Decentraland Wearables",
  land: "Decentraland Lands",
  other: "Decentraland Collectibles",
};

const Spinner = () => <span className="mkbuyflow__spinner" aria-hidden="true" />;

type MkBuyFlowProps = LabelSuffixProps & {
  asset?: BuyAsset;
  chainName?: string;
  chainHue?: number;
  tokenSymbol?: string;
  tokenBalance?: string;
  itemCostToken?: string;
  itemCostUsd?: string;
  feeCostToken?: string;
  feeCostUsd?: string;
  totalToken?: string;
  totalUsd?: string;
  crossChain?: boolean;
  exchangeRate?: string;
  duration?: string;
  showFeeCovered?: boolean;
  state?: BuyState;
  chains?: ChainOption[];
  tokens?: TokenOption[];
  /** Forwarded to `Modal`. `false` renders the dialog in place instead of portalling it. */
  portal?: boolean;
  onChainChange?: (id: string) => void;
  onTokenChange?: (symbol: string) => void;
  onPrimary?: () => void;
  onBack?: () => void;
  onClose?: () => void;
  onGetMana?: () => void;
  onContinueCard?: () => void;
};

export default function MkBuyFlow({
  asset = EMPTY_ASSET,
  chainName: chainNameProp = "Polygon",
  chainHue: chainHueProp = 268,
  tokenSymbol: tokenSymbolProp = "MANA",
  tokenBalance: tokenBalanceProp = "",
  itemCostToken = "",
  itemCostUsd = "",
  feeCostToken = "",
  feeCostUsd = "",
  totalToken = "",
  totalUsd = "",
  crossChain = false,
  exchangeRate = "",
  duration = "",
  showFeeCovered = true,
  state = "default",
  chains = DEFAULT_CHAINS,
  tokens,
  portal = true,
  labelSuffix,
  onChainChange,
  onTokenChange,
  onPrimary,
  onBack,
  onClose,
  onGetMana,
  onContinueCard,
}: MkBuyFlowProps) {
  const [showCard, setShowCard] = useState(state === "card");

  const chainList = chains.length ? chains : DEFAULT_CHAINS;
  const tokenList = tokens ?? defaultTokens(tokenBalanceProp);
  const [chainId, setChainId] = useState(
    () => chainList.find((c) => c.name === chainNameProp)?.id ?? chainList[0]?.id ?? "polygon",
  );
  const [tokenSym, setTokenSym] = useState(tokenSymbolProp);
  const chain = chainList.find((c) => c.id === chainId) ?? {
    id: chainId,
    name: chainNameProp,
    hue: chainHueProp,
  };
  const token = tokenList.find((t) => t.symbol === tokenSym) ?? {
    symbol: tokenSym,
    balance: tokenBalanceProp,
    mana: tokenSym === "MANA",
    hue: chainHueProp,
  };
  const chainName = chain.name;
  const chainHue = chain.hue;
  const tokenSymbol = token.symbol;
  const tokenBalance = token.balance ?? tokenBalanceProp;

  function pickChain(id: string) {
    setChainId(id);
    onChainChange?.(id);
  }
  function pickToken(symbol: string) {
    setTokenSym(symbol);
    onTokenChange?.(symbol);
  }

  const isBuying = state === "buying";
  const isLoadingRoute = state === "loadingRoute";
  const isMana = token.mana ?? (tokenSymbol === "MANA");

  const description = ASSET_DESCRIPTION[asset.kind] || ASSET_DESCRIPTION.other;
  const isEns = asset.kind === "ens";

  if (showCard) {
    return (
      <Modal width={440} className="modal__card--plain mkbuyflow__modal mkbuyflow__modal--card" ariaLabel="Buy with card" portal={portal}>
          <nav className="mkbuyflow__nav" aria-label={labelSuffix ? suffixLabel("Buy with card", labelSuffix) : undefined}>
            <span className="mkbuyflow__navtitle">Buy with card</span>
            <button type="button" className="mkbuyflow__close" aria-label="Close" onClick={() => setShowCard(false)}>
              &#x2715;
            </button>
          </nav>
          <div className="mkbuyflow__cardbody">
            <p>
              Card payments are processed by{" "}
              <a href="https://transak.com/" target="_blank" rel="noopener noreferrer">
                Transak
              </a>
              .{"\n"}Transak charges a fee for its service. You will see the final price before confirming the purchase.
            </p>
            <p className="mkbuyflow__learnmore">
              <a href="https://transak.com/nft-checkout" target="_blank" rel="noopener noreferrer">
                Learn more about card payments
              </a>
            </p>
          </div>
          <div className="mkbuyflow__actions mkbuyflow__actions--stack">
            <button type="button" className="mkbuyflow__btn mkbuyflow__btn--primary" onClick={onContinueCard}>
              Continue
            </button>
            <button type="button" className="mkbuyflow__btn mkbuyflow__btn--secondary" onClick={() => setShowCard(false)}>
              Go back
            </button>
          </div>
      </Modal>
    );
  }

  const assetImgStyle: CSSProperties & { "--rb": string } = {
    "--rb": `var(--rar-bg-${asset.rarity})`,
  };

  return (
    <Modal width={440} className="modal__card--plain mkbuyflow__modal" ariaLabel="Confirm Your Purchase" portal={portal}>
        <nav className="mkbuyflow__nav" aria-label={labelSuffix ? suffixLabel("Confirm Your Purchase", labelSuffix) : undefined}>
          {!isBuying ? (
            <button type="button" className="mkbuyflow__back" aria-label="Back" onClick={onBack}>
              <ChevronLeft size={16} />
            </button>
          ) : null}
          <span className="mkbuyflow__navtitle">Confirm Your Purchase</span>
          {!isBuying ? (
            <button type="button" className="mkbuyflow__close" aria-label="Close" onClick={onClose}>
              &#x2715;
            </button>
          ) : null}
        </nav>

        <div className="mkbuyflow__content">
          <div className="mkbuyflow__assetrow">
            <div
              className="mkbuyflow__assetimg u-rar-bg"
              style={assetImgStyle}
              aria-hidden="true"
            >
              {asset.image ? (
                <img
                  src={asset.image}
                  alt=""
                  style={{ width: "100%", height: "100%", objectFit: "contain", display: "block", borderRadius: "inherit", position: "relative", zIndex: 1 }}
                />
              ) : (
                <ManaMark size={22} />
              )}
            </div>
            <div className="mkbuyflow__assetdetails">
              <span className="mkbuyflow__assetname">
                {isEns ? (
                  <>
                    <strong>{asset.name}</strong>.dcl.eth
                  </>
                ) : (
                  asset.name
                )}
              </span>
              <span className="mkbuyflow__assetdesc">{description}</span>
            </div>
            <div className="mkbuyflow__price">
              <span className="mkbuyflow__pricemana">
                <ManaMark size={18} />
                {asset.priceMana}
              </span>
              {asset.priceUsd ? (
                <span className="mkbuyflow__priceusd">${asset.priceUsd} USD</span>
              ) : null}
            </div>
          </div>

          <div className="mkbuyflow__paywith">
            <div className="mkbuyflow__selectors">
              <div className="mkbuyflow__selectorcol">
                <span className="mkbuyflow__paylabel">Pay with</span>
                <SelectorMenu
                  ariaLabel="Select network"
                  className="mkbuyflow__selector"
                  disabled={isBuying}
                  current={chainId}
                  onSelect={pickChain}
                  trigger={
                    <>
                      <TokenIcon size={25} hue={chainHue} />
                      <span className="mkbuyflow__selname">{chainName}</span>
                    </>
                  }
                  options={chainList.map((c) => ({
                    id: c.id,
                    content: (
                      <>
                        <TokenIcon size={22} hue={c.hue} />
                        <span className="mkbuyflow__optname">{c.name}</span>
                      </>
                    ),
                  }))}
                />
              </div>
              <div className="mkbuyflow__selectorcol mkbuyflow__selectorcol--token">
                <SelectorMenu
                  ariaLabel="Select token"
                  className="mkbuyflow__selector mkbuyflow__selector--token"
                  disabled={isBuying}
                  hideChevronWhenDisabled
                  current={tokenSymbol}
                  onSelect={pickToken}
                  trigger={
                    <>
                      <TokenIcon size={25} mana={isMana} hue={chainHue} />
                      <span className="mkbuyflow__selname">{tokenSymbol}</span>
                      {tokenBalance ? (
                        <span className="mkbuyflow__balance">
                          Balance: <span className="mkbuyflow__balanceval">{tokenBalance}</span>
                        </span>
                      ) : null}
                    </>
                  }
                  options={tokenList.map((t) => ({
                    id: t.symbol,
                    content: (
                      <>
                        <TokenIcon size={22} mana={t.mana} hue={t.hue ?? chainHue} />
                        <span className="mkbuyflow__optname">{t.symbol}</span>
                        {t.balance != null ? (
                          <span className="mkbuyflow__optbal">{t.balance}</span>
                        ) : null}
                      </>
                    ),
                  }))}
                />
              </div>
            </div>

            <div className="mkbuyflow__costs">
              <div className="mkbuyflow__costrow">
                <div className="mkbuyflow__costlabel">Item Cost</div>
                <div className="mkbuyflow__costamount">
                  <span className="mkbuyflow__costtoken">
                    <TokenIcon size={24} mana={isMana} hue={chainHue} />
                    {itemCostToken}
                  </span>
                  {itemCostUsd ? <span className="mkbuyflow__costusd">&#x2248; ${itemCostUsd}</span> : null}
                </div>
              </div>
              <div className="mkbuyflow__costrow">
                <div className="mkbuyflow__costlabel mkbuyflow__costlabel--fee">
                  Estimated Fee
                  <span className="mkbuyflow__info u-tip">
                    <InfoMark />
                    <span className="u-tip__bubble">
                      Estimated fee includes the network cost that you have to pay directly with your wallet.
                    </span>
                  </span>
                </div>
                <div className="mkbuyflow__costamount">
                  <span className="mkbuyflow__costtoken">
                    <TokenIcon size={24} mana={isMana} hue={chainHue} />
                    {feeCostToken}
                  </span>
                  {feeCostUsd ? <span className="mkbuyflow__costusd">&#x2248; ${feeCostUsd}</span> : null}
                </div>
              </div>
            </div>
          </div>

          <div className="mkbuyflow__total">
            <div className="mkbuyflow__totalleft">
              <span className="mkbuyflow__totallabel">Total</span>
              {showFeeCovered ? (
                <span className="mkbuyflow__feecovered">
                  Gas fees are <span className="mkbuyflow__feefree">covered by the DAO</span> when paying with MANA.
                </span>
              ) : null}
            </div>
            <div className="mkbuyflow__totalright">
              {isLoadingRoute ? (
                <span className="mkbuyflow__skel mkbuyflow__skel--total" />
              ) : (
                <span className="mkbuyflow__totaltoken">
                  <TokenIcon size={24} mana={isMana} hue={chainHue} />
                  {totalToken}
                </span>
              )}
              {totalUsd ? <span className="mkbuyflow__totalusd">${totalUsd} USD</span> : null}
            </div>
          </div>

          {crossChain ? (
            <div className="mkbuyflow__duration">
              <div className="mkbuyflow__durrow">
                <span className="mkbuyflow__durlabel">
                  <ClockMark /> Transaction Duration
                </span>
                <span>{duration}</span>
              </div>
              <div className="mkbuyflow__durrow mkbuyflow__exchangerow">
                <span className="mkbuyflow__durlabel">
                  <span className="mkbuyflow__exchangeicon" aria-hidden="true" /> Exchange Rate
                </span>
                <span>
                  1 {tokenSymbol} = {exchangeRate} MANA
                </span>
              </div>
            </div>
          ) : null}

          {showFeeCovered && asset.network === "MATIC" && !crossChain ? (
            <span className="mkbuyflow__remember">
              Pay with Polygon MANA to have gas fees{" "}
              <span className="mkbuyflow__feefree">covered for you by the DAO</span> (item must be at least 1 MANA).
            </span>
          ) : null}

          {state === "priceTooLow" ? (
            <span className="mkbuyflow__warning">
              MANA transactions are only gas fee free if the item is at least 1 MANA. To get this item, switch your
              network to Polygon to pay for the gas fee with MATIC.{" "}
              <a href="https://docs.decentraland.org" target="_blank" rel="noreferrer">
                <u>Learn More</u>
              </a>
            </span>
          ) : null}

          {state === "insufficient" ? (
            <span className="mkbuyflow__warning">
              You don&#x2019;t have enough funds in {tokenSymbol} to pay for this item. Get MANA, or pay with a different token,
              or pay by card.
            </span>
          ) : null}

          {state === "routeUnavailable" ? (
            <span className="mkbuyflow__warning">
              Buying with {tokenSymbol} is not available at the moment. Get MANA, pay with a different token, or pay by
              card.
            </span>
          ) : null}
        </div>

        <div className="mkbuyflow__actions">
          {state === "insufficient" || state === "routeUnavailable" ? (
            <>
              <button type="button" className="mkbuyflow__btn mkbuyflow__btn--primary" onClick={onGetMana}>
                Get MANA
              </button>
              <button
                type="button"
                className="mkbuyflow__btn mkbuyflow__btn--secondary"
                onClick={() => setShowCard(true)}
              >
                <span className="mkbuyflow__cardicon" aria-hidden="true" />
                Buy with card
              </button>
            </>
          ) : (
            <button
              type="button"
              className={"mkbuyflow__btn mkbuyflow__btn--primary" + (isBuying || isLoadingRoute ? " is-loading" : "")}
              disabled={isBuying || isLoadingRoute}
              onClick={onPrimary}
              aria-label={isLoadingRoute && !isBuying ? "Buy now" : undefined}
            >
              {isBuying ? (
                <>
                  <Spinner /> Confirm Transaction in Your Wallet
                </>
              ) : isLoadingRoute ? (
                <Spinner />
              ) : (
                "Buy now"
              )}
            </button>
          )}
        </div>
    </Modal>
  );
}
