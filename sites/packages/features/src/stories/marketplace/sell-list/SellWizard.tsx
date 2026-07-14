import { useEffect, useRef, useState, type ComponentProps } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import MarketplaceChrome, { type MarketplaceNavId } from "@ui/marketplace/frames/MarketplaceChrome";
import AssetActionLayout from "@ui/marketplace/frames/AssetActionLayout";
import AssetPreviewTile from "@ui/marketplace/components/AssetPreviewTile";
import ManaMark from "@ui/atoms/ManaMark";
import Button from "@ui/atoms/Button";
import Spinner from "@ui/atoms/Spinner";
import Web3Confirm from "@ui/web/workflows/Web3Confirm";
import MkSuccessPage from "@ui/marketplace/pages/MkSuccessPage";
import AssetCard from "@ui/marketplace/components/AssetCard";
import "@ui/marketplace/pages/mksellpage.css";
import "@ui/marketplace/frames/assetactionlayout.css";
import "@ui/marketplace/components/assetcard.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  toSellNft,
  ownedAssetImage,
  type CreateOrderFn,
  type OwnedAsset,
} from "@data/lib/catalyst/marketplace/sell";
import { weiToMana } from "@data/lib/catalyst/marketplace/money";
import {
  sellMachine,
  resolveSellSnapshot,
  slugToState,
  stateToSlug,
  isValidPrice,
  type TrackFn,
} from "./machine";

function ui3<C extends React.ComponentType<any>>(
  props: Partial<ComponentProps<C>>,
): ComponentProps<C> {
  return props as ComponentProps<C>;
}

export type SellWizardProps = {
  trackCtx: TrackContext;
  assets: OwnedAsset[];
  initialStep?: string;
  createOrder?: CreateOrderFn;
  track?: TrackFn;
};

export default function SellWizard({
  trackCtx,
  assets,
  initialStep,
  createOrder,
  track,
}: SellWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <SellWizardInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      assets={assets}
      createOrder={createOrder}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  trackCtx: TrackContext;
  assets: OwnedAsset[];
  createOrder?: CreateOrderFn;
  track?: TrackFn;
};

const MIN_EXPIRATION = "2026-07-20";

const noop = () => {};

function SellWizardInner({
  stateId,
  trackCtx,
  assets,
  createOrder,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveSellSnapshot({ step: stateId, trackCtx, assets, createOrder, track }),
  ).current;

  const [state, send] = useMachine(sellMachine, {
    input: { trackCtx, assets, createOrder, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);

  const [priceInput, setPriceInput] = useState("1000");
  const [expirationInput, setExpirationInput] = useState(MIN_EXPIRATION);
  const [tab, setTab] = useState<MarketplaceNavId>("my-assets");

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams]);

  const selectedAsset =
    assets.find((a) => a.id === state.context.assetId) ?? assets[0] ?? null;
  const sellNft = selectedAsset ? toSellNft(selectedAsset) : null;

  const priceNum = parseFloat(priceInput.replace(/,/g, ""));
  const priceOk = isValidPrice(priceNum);
  const expInvalid =
    !!expirationInput &&
    new Date(`${expirationInput} 00:00:00`).getTime() < Date.now();

  function submitPrice() {
    send({ type: "SET_PRICE", priceMana: priceNum });
  }

  function submitExpiration() {
    if (expInvalid) return;
    const expiresAt = new Date(`${expirationInput} 00:00:00`).getTime();
    send({ type: "SET_EXPIRATION", expiresAt });
  }

  return (
    <div className="sell-wizard" data-step={step}>
      <MarketplaceChrome active={tab} onTab={setTab}>
        {value === "selectAsset" && (
          <div className="sell-wizard__pick mksellpage">
            <div className="mksellpage__page">
              <h1 className="mksellpage__title">List an item for sale</h1>
              <p className="mksellpage__subtitle">
                Choose one of your owned items to create a listing.
              </p>
              {assets.length > 0 ? (
                <div
                  className="sell-wizard__grid"
                  style={{
                    display: "grid",
                    gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
                    gap: 16,
                    marginTop: 16,
                  }}
                >
                  {assets.map((a) => (
                    <button
                      key={a.id}
                      type="button"
                      className="sell-wizard__assetbtn"
                      onClick={() => send({ type: "SELECT_ASSET", assetId: a.id })}
                      aria-label={`List ${a.name ?? a.id}`}
                      style={{
                        background: "none",
                        border: "none",
                        padding: 0,
                        cursor: "pointer",
                        textAlign: "left",
                      }}
                    >
                      <AssetCard
                        {...ui3<typeof AssetCard>({
                          name: a.name ?? "Untitled",
                          rarity: a.rarity ?? "common",
                          network: a.network === "ETHEREUM" ? "ethereum" : "polygon",
                          price: a.activeOrderId ? "Listed" : undefined,
                          image: ownedAssetImage(a),
                        })}
                      />
                    </button>
                  ))}
                </div>
              ) : (
                <div style={{ marginTop: 16 }}>
                  <p style={{ color: "rgba(255,255,255,0.7)", margin: "0 0 12px" }}>
                    You don&apos;t own anything listable yet &#x2014; items you buy
                    appear here, ready to resell.
                  </p>
                  <a href="/shop" className="btn btn--primary btn--md">
                    Browse the shop
                  </a>
                </div>
              )}
            </div>
          </div>
        )}

        {value === "setPrice" && sellNft && (
          <div className="mksellpage">
            <div className="mksellpage__page">
              <button
                type="button"
                className="mksellpage__back"
                onClick={() => send({ type: "BACK" })}
              >
                Back
              </button>
              <div className="mksellpage__row">
                <div className="mksellpage__left">
                  <AssetPreviewTile {...ui3<typeof AssetPreviewTile>({ rarity: sellNft.rarity })} />
                </div>
                <div className="mksellpage__right">
                  <h1 className="mksellpage__title">List for sale</h1>
                  <p className="mksellpage__subtitle">
                    Set a price for <b>{sellNft.name}</b>.
                  </p>
                  <form
                    onSubmit={(e) => {
                      e.preventDefault();
                      submitPrice();
                    }}
                  >
                    <div className="mksellpage__field">
                      <label className="mksellpage__label" htmlFor="sell-price">
                        Price
                      </label>
                      <div
                        className={
                          "mksellpage__inputbox" +
                          (priceInput !== "" && !priceOk ? " is-error" : "")
                        }
                      >
                        <span className="mksellpage__mana">
                          <ManaMark {...ui3<typeof ManaMark>({})} />
                        </span>
                        <input
                          id="sell-price"
                          className="mksellpage__input"
                          type="text"
                          inputMode="decimal"
                          placeholder="1000"
                          autoFocus
                          value={priceInput}
                          onChange={(e) => setPriceInput(e.target.value)}
                        />
                      </div>
                      {priceInput !== "" && !priceOk ? (
                        <p className="mksellpage__msg">
                          Enter a price greater than 0 MANA.
                        </p>
                      ) : null}
                    </div>
                    <div className="mksellpage__buttons">
                      <Button
                        type="button"
                        variant="secondary"
                        className="mksellpage__cancel"
                        onClick={() => send({ type: "BACK" })}
                      >
                        Cancel
                      </Button>
                      <Button type="submit" variant="primary" onClick={noop}>
                        Continue
                      </Button>
                    </div>
                  </form>
                </div>
              </div>
            </div>
          </div>
        )}

        {value === "setExpiration" && sellNft && (
          <div className="mksellpage">
            <div className="mksellpage__page">
              <button
                type="button"
                className="mksellpage__back"
                onClick={() => send({ type: "BACK" })}
              >
                Back
              </button>
              <div className="mksellpage__row">
                <div className="mksellpage__left">
                  <AssetPreviewTile {...ui3<typeof AssetPreviewTile>({ rarity: sellNft.rarity })} />
                </div>
                <div className="mksellpage__right">
                  <h1 className="mksellpage__title">Set expiration</h1>
                  <p className="mksellpage__subtitle">
                    Choose how long the listing for <b>{sellNft.name}</b> stays
                    active.
                  </p>
                  <form
                    onSubmit={(e) => {
                      e.preventDefault();
                      submitExpiration();
                    }}
                  >
                    <div className="mksellpage__field">
                      <label className="mksellpage__label" htmlFor="sell-exp">
                        Expiration date
                      </label>
                      <div
                        className={
                          "mksellpage__inputbox" + (expInvalid ? " is-error" : "")
                        }
                      >
                        <input
                          id="sell-exp"
                          className="mksellpage__input"
                          type="date"
                          value={expirationInput}
                          onChange={(e) => setExpirationInput(e.target.value)}
                        />
                      </div>
                      {expInvalid ? (
                        <p className="mksellpage__msg">This date has already passed</p>
                      ) : null}
                    </div>
                    <div className="mksellpage__buttons">
                      <Button
                        type="button"
                        variant="secondary"
                        className="mksellpage__cancel"
                        onClick={() => send({ type: "BACK" })}
                      >
                        Back
                      </Button>
                      <Button type="submit" variant="primary" disabled={expInvalid} onClick={noop}>
                        Continue
                      </Button>
                    </div>
                  </form>
                </div>
              </div>
            </div>
          </div>
        )}

        {value === "approveNft" && sellNft && (
          <AssetActionLayout
            theme="dark"
            variant="status"
            title="Approve your item"
            iconTone="neutral"
            media={null}
            warning={null}
            icon={<Spinner {...ui3<typeof Spinner>({ size: 28 })} />}
            subtitle={
              <>
                Allow the Marketplace contract to transfer <b>{sellNft.name}</b>{" "}
                when it sells. This is a one-time approval per collection.
                <br />
                <small style={{ opacity: 0.7 }}>
                  Your wallet will ask you to confirm an on-chain approval transaction.
                </small>
              </>
            }
            onBack={() => send({ type: "BACK" })}
          >
            <div className="sell-wizard__controls" style={{ display: "flex", gap: 12, marginTop: 20 }}>
              <Button variant="secondary" onClick={() => send({ type: "BACK" })}>
                Back
              </Button>
              <Button variant="primary" onClick={() => send({ type: "APPROVE" })}>
                Approve NFT
              </Button>
            </div>
          </AssetActionLayout>
        )}

        {value === "signOrder" && (
          <div className="sell-wizard__sign">
            <Web3Confirm
              code="SELL-712"
              expiry="05:00"
              onBack={() => send({ type: "BACK" })}
              onExit={() => send({ type: "BACK" })}
            />
            <div
              className="sell-wizard__controls"
              style={{ display: "flex", gap: 12, justifyContent: "center", padding: 24 }}
            >
              <Button variant="secondary" onClick={() => send({ type: "BACK" })}>
                Back
              </Button>
              <Button variant="primary" onClick={() => send({ type: "SIGN" })}>
                Sign order
              </Button>
            </div>
          </div>
        )}

        {value === "confirm" && (
          <AssetActionLayout
            theme="dark"
            variant="status"
            hideBack
            onBack={noop}
            media={null}
            warning={null}
            title="Creating your listing"
            iconTone="neutral"
            icon={<Spinner {...ui3<typeof Spinner>({ size: 28 })} />}
            subtitle={
              <>
                Submitting the sell order to the Marketplace.
                <br />
                <small style={{ opacity: 0.7 }}>
                  Approve the item, then sign the listing in your wallet.
                </small>
              </>
            }
          >
            <span />
          </AssetActionLayout>
        )}

        {value === "success" && sellNft && (
          <SellSuccess
            nft={sellNft}
            priceMana={
              state.context.priceMana ??
              (state.context.result
                ? weiToMana(state.context.result.order.price)
                : 0)
            }
            orderId={state.context.result?.order.id ?? ""}
            approvalTxHash={state.context.result?.approvalTxHash ?? null}
          />
        )}

        {value === "error" && (
          <div className="sell-wizard__error">
            <MkSuccessPage
              state="error"
              asset={{
                category: sellNft?.category ?? "wearable",
                name: sellNft?.name ?? "item",
                rarity: sellNft?.rarity ?? "common",
              }}
            />
            <div
              className="sell-wizard__controls"
              style={{ display: "flex", gap: 12, justifyContent: "center", padding: 24 }}
            >
              <Button variant="primary" onClick={() => send({ type: "RETRY" })}>
                Retry
              </Button>
            </div>
          </div>
        )}
      </MarketplaceChrome>
    </div>
  );
}

function SellSuccess({
  nft,
  priceMana,
  orderId,
  approvalTxHash,
}: {
  nft: { name: string; category: string; rarity: string };
  priceMana: number;
  orderId: string;
  approvalTxHash: string | null;
}) {
  return (
    <div className="sell-wizard__success">
      <MkSuccessPage
        state="success"
        asset={{ category: nft.category, name: nft.name, rarity: nft.rarity }}
      />
      <div
        className="sell-wizard__receipt"
        role="status"
        style={{
          maxWidth: 560,
          margin: "0 auto",
          padding: 16,
          color: "rgba(255,255,255,0.85)",
          textAlign: "center",
        }}
      >
        Listed <strong>{nft.name}</strong> for{" "}
        <strong>{priceMana.toLocaleString()} MANA</strong>.
        <br />
        <small style={{ opacity: 0.65 }}>
          Order {orderId || "(pending)"} &#xB7;{" "}
          {approvalTxHash
            ? `approval tx ${approvalTxHash}`
            : "the marketplace was already approved"}
        </small>
      </div>
    </div>
  );
}
