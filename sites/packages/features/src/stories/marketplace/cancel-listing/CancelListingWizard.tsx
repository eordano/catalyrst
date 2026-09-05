import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import MkAssetPage from "@ui/marketplace/pages/MkAssetPage";
import MkCancelSalePage from "@ui/marketplace/pages/MkCancelSalePage";
import MkSuccessPage from "@ui/marketplace/pages/MkSuccessPage";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  cancelMachine,
  resolveCancelSnapshot,
  slugToState,
  stateToSlug,
  type CancelFn,
  type CancelOrder,
  type Ownership,
  type TrackFn,
} from "./machine";

export type CancelListingWizardProps = {
  trackCtx: TrackContext;
  order: CancelOrder;
  ownership?: Ownership;
  initialStep?: string;
  cancel?: CancelFn;
  track?: TrackFn;
};

export default function CancelListingWizard({
  trackCtx,
  order,
  ownership = "self",
  initialStep,
  cancel,
  track,
}: CancelListingWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <CancelListingWizardInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      order={order}
      ownership={ownership}
      cancel={cancel}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  trackCtx: TrackContext;
  order: CancelOrder;
  ownership: Ownership;
  cancel?: CancelFn;
  track?: TrackFn;
};

function CancelListingWizardInner({
  stateId,
  trackCtx,
  order,
  ownership,
  cancel,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveCancelSnapshot({ step: stateId, trackCtx, order, ownership, cancel, track }),
  ).current;

  const [state, send] = useMachine(cancelMachine, {
    input: { trackCtx, order, ownership, cancel, track },
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

  const cancelOrder = { price: order.price ?? "unknown", owner: "self" };
  const cancelNft = { name: order.name, category: "wearable", rarity: "legendary", network: order.network };

  return (
    <div className="cancel-listing-wizard" data-step={step}>
      {value === "reviewing" && (
        <>
          <MkAssetPage
            nft={{
              name: order.name,
              issuedId: 0,
              category: "wearable",
              rarity: "legendary",
              bodyShape: "Unisex",
              isSmart: false,
              network: order.network,
              description: order.price
                ? `This item is listed for sale for ${order.price} MANA. Remove the listing to take it off the marketplace.`
                : "This item is listed for sale; we could not read the listed price. Remove the listing to take it off the marketplace.",
              owner: { address: order.owner, name: "" },
              collection: { name: "Decentraland", address: "" },
              order: { price: order.price ?? "unknown", issuedId: 0, expiresLabel: "Active listing" },
            }}
            listings={[]}
            emptyListings
          />
          <div className="cancel-listing-wizard__controls" role="group" aria-label="Cancel listing">
            <button
              type="button"
              className="cancel-listing-wizard__btn cancel-listing-wizard__btn--primary"
              onClick={() => send({ type: "CONNECT_WALLET" })}
            >
              Remove listing
            </button>
            <button
              type="button"
              className="cancel-listing-wizard__btn"
              onClick={() => send({ type: "NOT_OWNER" })}
            >
              I am not the seller
            </button>
          </div>
        </>
      )}

      {value === "connecting" && (
        <>
          <MkCancelSalePage nft={cancelNft} order={cancelOrder} ownership="self" status="authorize" onConfirm={() => {}} onCancel={() => {}} />
          <div className="cancel-listing-wizard__controls" role="group" aria-label="Sign in">
            <button
              type="button"
              className="cancel-listing-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back
            </button>
            <button
              type="button"
              className="cancel-listing-wizard__btn cancel-listing-wizard__btn--primary"
              onClick={() => send({ type: "CONFIRM" })}
            >
              Sign in
            </button>
          </div>
        </>
      )}

      {value === "confirming" && (
        <>
          <MkCancelSalePage nft={cancelNft} order={cancelOrder} ownership="self" status="confirmation" onConfirm={() => {}} onCancel={() => {}} />
          <div className="cancel-listing-wizard__controls" role="group" aria-label="Confirm cancel">
            <button
              type="button"
              className="cancel-listing-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back
            </button>
            <button
              type="button"
              className="cancel-listing-wizard__btn cancel-listing-wizard__btn--primary"
              onClick={() => send({ type: "SUBMIT" })}
            >
              Confirm cancel (simulated)
            </button>
          </div>
        </>
      )}

      {value === "submitting" && (
        <MkCancelSalePage nft={cancelNft} order={cancelOrder} ownership="self" status="pending" onConfirm={() => {}} onCancel={() => {}} />
      )}

      {value === "success" && (
        <MkSuccessPage
          state="success"
          asset={{ category: "wearable", name: order.name, rarity: "legendary" }}
        />
      )}

      {value === "notOwner" && (
        <>
          <MkCancelSalePage
            nft={cancelNft}
            order={ownership === "none" ? undefined : cancelOrder}
            ownership={ownership === "none" ? "none" : "other"}
            status="confirmation"
            onConfirm={() => {}}
            onCancel={() => {}}
          />
          <div className="cancel-listing-wizard__controls">
            <button
              type="button"
              className="cancel-listing-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back
            </button>
          </div>
        </>
      )}

      {value === "error" && (
        <>
          <MkCancelSalePage nft={cancelNft} order={cancelOrder} ownership="self" status="confirmation" onConfirm={() => {}} onCancel={() => {}} />
          <div className="cancel-listing-wizard__controls">
            <p className="cancel-listing-wizard__error">{state.context.error ?? "Cancel failed."}</p>
            <button
              type="button"
              className="cancel-listing-wizard__btn cancel-listing-wizard__btn--primary"
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
