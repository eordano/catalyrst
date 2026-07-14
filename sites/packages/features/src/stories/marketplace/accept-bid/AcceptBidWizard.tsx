import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import MkAcceptBidWizardView from "@ui/marketplace/workflows/MkAcceptBidWizardView";

import type { Bid } from "@data/lib/catalyst/marketplace/bids";
import type { TrackContext } from "@core/lib/telemetry/track";
import {
  acceptBidMachine,
  resolveAcceptSnapshot,
  slugToState,
  stateToSlug,
  type AcceptFn,
  type ApproveFn,
  type TrackFn,
} from "./machine";

export type AcceptBidWizardProps = {
  trackCtx: TrackContext;
  bid: Bid;
  initialStep?: string;
  approve?: ApproveFn;
  accept?: AcceptFn;
  track?: TrackFn;
};

export default function AcceptBidWizard({
  trackCtx,
  bid,
  initialStep,
  approve,
  accept,
  track,
}: AcceptBidWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <AcceptBidWizardInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      bid={bid}
      approve={approve}
      accept={accept}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  trackCtx: TrackContext;
  bid: Bid;
  approve?: ApproveFn;
  accept?: AcceptFn;
  track?: TrackFn;
};

function AcceptBidWizardInner({
  stateId,
  trackCtx,
  bid,
  approve,
  accept,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveAcceptSnapshot({ step: stateId, trackCtx, bid, approve, accept, track }),
  ).current;

  const [state, send] = useMachine(acceptBidMachine, {
    input: { trackCtx, bid, approve, accept, track },
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

  return (
    <MkAcceptBidWizardView
      bid={bid}
      value={value}
      step={step}
      error={state.context.error}
      txHash={state.context.result?.txHash}
      onReject={() => send({ type: "REJECT" })}
      onAccept={() => send({ type: "ACCEPT" })}
      onBack={() => send({ type: "BACK" })}
      onConnect={() => send({ type: "CONNECT" })}
      onConfirm={() => send({ type: "CONFIRM" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
