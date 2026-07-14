import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import MkBuyFlow from "@ui/marketplace/workflows/MkBuyFlow";
import MkBuyStatusPage from "@ui/marketplace/pages/MkBuyStatusPage";
import MkSuccessPage from "@ui/marketplace/pages/MkSuccessPage";
import AcConvertMANAModal from "@ui/account/components/AcConvertMANAModal";
import Web3Confirm from "@ui/web/workflows/Web3Confirm";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  buyMachine,
  resolveBuySnapshot,
  slugToState,
  stateToSlug,
  type BuyListing,
  type SimFn,
  type TrackFn,
} from "./machine";

export type BuyWizardProps = {
  listing: BuyListing;
  display: {
    name: string;
    rarity: string;
    category: string;
    kind: "wearable" | "emote" | "ens" | "land" | "other";
    image?: string | null;
  };
  trackCtx: TrackContext;
  initialStep?: string;
  connect?: SimFn;
  approve?: SimFn;
  commit?: SimFn;
  track?: TrackFn;
};

export default function BuyWizard({
  listing,
  display,
  trackCtx,
  initialStep,
  connect,
  approve,
  commit,
  track,
}: BuyWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const TRANSIENT = new Set(["connecting", "approving", "submitting"]);
  const rawState = slugToState(urlStep);
  const stateId = TRANSIENT.has(rawState) ? "review" : rawState;

  return (
    <BuyWizardInner
      stateId={stateId}
      listing={listing}
      display={display}
      trackCtx={trackCtx}
      connect={connect}
      approve={approve}
      commit={commit}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  listing: BuyListing;
  display: BuyWizardProps["display"];
  trackCtx: TrackContext;
  connect?: SimFn;
  approve?: SimFn;
  commit?: SimFn;
  track?: TrackFn;
};

function BuyWizardInner({
  stateId,
  listing,
  display,
  trackCtx,
  connect,
  approve,
  commit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveBuySnapshot({
      step: stateId,
      listing,
      trackCtx,
      connect,
      approve,
      commit,
      track,
    }),
  ).current;

  const [state, send] = useMachine(buyMachine, {
    input: { listing, trackCtx, connect, approve, commit, track },
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

  const flowAsset = {
    name: display.name,
    rarity: display.rarity,
    category: display.category,
    network: listing.network === "ethereum" ? "ETHEREUM" : "MATIC",
    kind: display.kind,
    priceMana: listing.priceMana,
    priceUsd: "",
    image: display.image ?? null,
  };
  const successCategory = display.kind === "land" ? "parcel" : display.kind;
  const successAsset = {
    category: successCategory,
    name: display.name,
    rarity: display.rarity,
  };

  return (
    <div className="buy-wizard" data-step={step}>
      {value === "review" && (
        <MkBuyFlow
          asset={flowAsset}
          tokenSymbol="MANA"
          itemCostToken={listing.priceMana}
          totalToken={listing.priceMana}
          state="default"
          onPrimary={() => send({ type: "START" })}
          onBack={() => send({ type: "CANCEL" })}
          onClose={() => send({ type: "CANCEL" })}
        />
      )}

      {value === "connecting" && (
        <Web3Confirm code="DCL-SIM" expiry="05:00" onBack={() => {}} onExit={() => {}} />
      )}

      {value === "approving" && (
        <AcConvertMANAModal stage="auth" network={listing.network === "ethereum" ? "ethereum" : "matic"} />
      )}

      {value === "confirming" && (
        <MkBuyFlow
          asset={flowAsset}
          tokenSymbol="MANA"
          itemCostToken={listing.priceMana}
          totalToken={listing.priceMana}
          state="default"
          onPrimary={() => send({ type: "CONFIRM" })}
          onBack={() => send({ type: "CANCEL" })}
          onClose={() => send({ type: "CANCEL" })}
        />
      )}

      {value === "submitting" && (
        <MkBuyStatusPage status="pending" asset={flowAsset} />
      )}

      {value === "success" && <MkSuccessPage state="success" asset={successAsset} />}

      {value === "error" && (
        <>
          <MkBuyStatusPage status="failed" asset={flowAsset} />
          <div className="buy-wizard__controls">
            {state.context.error ? (
              <p
                role="alert"
                style={{
                  color: "#ff9d9d",
                  margin: 0,
                  alignSelf: "center",
                  flexBasis: "100%",
                  textAlign: "center",
                }}
              >
                {state.context.error}
              </p>
            ) : null}
            <button
              type="button"
              className="buy-wizard__btn buy-wizard__btn--primary"
              onClick={() => send({ type: "RETRY" })}
            >
              Try again
            </button>
          </div>
        </>
      )}
    </div>
  );
}
