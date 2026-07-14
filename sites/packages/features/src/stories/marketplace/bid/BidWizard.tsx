import type { ReactNode } from "react";
import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import MkBidPage from "@ui/marketplace/pages/MkBidPage";
import MkBidPage2 from "@ui/marketplace/pages/MkBidPage2";
import MkSuccessPage from "@ui/marketplace/pages/MkSuccessPage";
import Web3Confirm from "@ui/web/workflows/Web3Confirm";

import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "../../../components/auth/signin-store";
import type { TrackContext } from "@core/lib/telemetry/track";
import {
  bidMachine,
  resolveBidSnapshot,
  slugToState,
  stateToSlug,
  type ChainFn,
  type TrackFn,
} from "./machine";

export type BidAsset = {
  id: string;
  name: string;
  category: string;
  rarity: string;
  network: "ethereum" | "polygon";
  floorMana: number | null;
  image: string | null;
};

export type BidWizardProps = {
  asset: BidAsset;
  trackCtx: TrackContext;
  manaBalance?: number;
  initialStep?: string;
  chain?: ChainFn;
  track?: TrackFn;
  banner?: ReactNode;
  onExit?: () => void;
};

export default function BidWizard({
  asset,
  trackCtx,
  manaBalance = 50000,
  initialStep,
  chain,
  track,
  banner,
  onExit,
}: BidWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const TRANSIENT = new Set(["approveMana", "signing", "confirming"]);
  const rawState = slugToState(urlStep);
  const stateId = TRANSIENT.has(rawState) ? "asset" : rawState;

  return (
    <BidWizardInner
      stateId={stateId}
      asset={asset}
      trackCtx={trackCtx}
      manaBalance={manaBalance}
      chain={chain}
      track={track}
      banner={banner}
      onExit={onExit}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  asset: BidAsset;
  trackCtx: TrackContext;
  manaBalance: number;
  chain?: ChainFn;
  track?: TrackFn;
  banner?: ReactNode;
  onExit?: () => void;
};

function BidWizardInner({
  stateId,
  asset,
  trackCtx,
  manaBalance,
  chain,
  track,
  banner,
  onExit,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const auth = useAuth();
  const bidAuthProps = {
    requiresSignIn: !auth.isConnected,
    onSignIn: openSignIn,
  };

  const snapshot = useRef(
    resolveBidSnapshot({ step: stateId, trackCtx, manaBalance, chain, track }),
  ).current;

  const [state, send] = useMachine(bidMachine, {
    input: { trackCtx, manaBalance, chain, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);

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

  function submitBid(rawPrice: string, expiration: string) {
    if (!auth.isConnected) {
      openSignIn();
      return;
    }
    const price = parseFloat(String(rawPrice).replace(/,/g, ""));
    if (!(price > 0)) return;
    if (value === "asset") send({ type: "REVIEW" });
    if (value === "insufficient" || value === "setExpiration")
      send({ type: "BACK" });
    send({ type: "SET_AMOUNT", price });
    send({ type: "SET_EXPIRATION", expiration });
  }

  const nft = {
    name: asset.name,
    category: asset.category,
    rarity: asset.rarity,
    network: asset.network,
  };
  const item = {
    name: asset.name,
    collection: asset.category,
    rarity: asset.rarity,
    network: asset.network === "ethereum" ? "ETHEREUM" : "MATIC",
    image: asset.image,
  };
  const price = state.context.price ?? asset.floorMana ?? 1000;
  const successAsset = {
    category: asset.category,
    name: asset.name,
    rarity: asset.rarity,
  };

  return (
    <div className="bid-wizard" data-step={step}>
      {value === "asset" && (
        <MkBidPage2
          item={item}
          banner={banner}
          onCancel={onExit}
          onBid={({ price: p, expiresAt }) => submitBid(p, expiresAt)}
        />
      )}

      {value === "setAmount" && (
        <MkBidPage
          nft={nft}
          manaBalance={manaBalance}
          banner={banner}
          onCancel={onExit}
          onSubmit={({ price: p, expiration }) => submitBid(p, expiration)}
          {...bidAuthProps}
        />
      )}

      {value === "insufficient" && (
        <MkBidPage
          nft={nft}
          manaBalance={manaBalance}
          initialPrice={String(state.context.price ?? "")}
          banner={banner}
          onCancel={onExit}
          onSubmit={({ price: p, expiration }) => submitBid(p, expiration)}
          {...bidAuthProps}
        />
      )}

      {value === "setExpiration" && (
        <MkBidPage
          nft={nft}
          manaBalance={manaBalance}
          initialPrice={String(price)}
          banner={banner}
          onCancel={onExit}
          onSubmit={({ price: p, expiration }) => submitBid(p, expiration)}
          {...bidAuthProps}
        />
      )}

      {value === "approveMana" && (
        <div className="bid-wizard__web3" aria-label="Approve MANA (simulated)">
          <Web3Confirm
            code="APPROVE"
            expiry="05:00"
            onBack={() => send({ type: "BACK" })}
            onExit={onExit ?? (() => {})}
          />
        </div>
      )}

      {value === "signing" && (
        <div className="bid-wizard__web3" aria-label="Sign bid (simulated)">
          <Web3Confirm code="SIGNBID" expiry="05:00" onBack={() => {}} onExit={onExit ?? (() => {})} />
        </div>
      )}

      {value === "confirming" && (
        <MkSuccessPage state="loading" asset={successAsset} />
      )}

      {value === "success" && <MkSuccessPage state="success" asset={successAsset} />}

      {value === "failed" && (
        <>
          <MkSuccessPage state="error" asset={successAsset} />
          <div className="bid-wizard__controls bid-wizard__controls--overlay">
            <button
              type="button"
              className="bid-wizard__btn"
              onClick={onExit}
            >
              Cancel
            </button>
            <button
              type="button"
              className="bid-wizard__btn bid-wizard__btn--primary"
              onClick={() => send({ type: "RETRY" })}
            >
              Retry
            </button>
          </div>
        </>
      )}
    </div>
  );
}
