import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import MkClaimNameWizardView from "@ui/marketplace/workflows/MkClaimNameWizardView";

import { openSignIn } from "../../../components/auth/signin-store";
import { useAuth } from "@data/lib/auth/index";
import { track as trackEvent } from "@core/lib/telemetry/track";
import type { TrackContext } from "@core/lib/telemetry/track";
import {
  claimNameMachine,
  resolveClaimSnapshot,
  slugToState,
  stateToSlug,
  type CheckAvailabilityFn,
  type MintFn,
  type ApproveFn,
  type TrackFn,
} from "./machine";

type ClaimNameWizardProps = {
  trackCtx: TrackContext;
  takenNames?: string[];
  sampleName?: string;
  initialStep?: string;
  allowStepPreview?: boolean;
  check?: CheckAvailabilityFn;
  mint?: MintFn;
  approve?: ApproveFn;
  track?: TrackFn;
  banner?: ReactNode;
  creditsNote?: string;
  onReturnToPublish?: (worldName: string) => void;
};

export default function ClaimNameWizard({
  trackCtx,
  takenNames = [],
  sampleName = "myWorld",
  initialStep,
  allowStepPreview = true,
  check,
  mint,
  approve,
  track,
  banner,
  creditsNote,
  onReturnToPublish,
}: ClaimNameWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = allowStepPreview ? (searchParams.get("step")?.trim() || initialStep) ?? undefined : undefined;
  const TRANSIENT = new Set(["checking", "approvalPending", "submitting"]);
  const rawState = slugToState(urlStep);
  const stateId = TRANSIENT.has(rawState) ? "entering" : rawState;

  return (
    <ClaimNameWizardInner
      stateId={stateId}
      trackCtx={trackCtx}
      takenNames={takenNames}
      sampleName={sampleName}
      check={check}
      mint={mint}
      approve={approve}
      track={track}
      banner={banner}
      creditsNote={creditsNote}
      onReturnToPublish={onReturnToPublish}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  trackCtx: TrackContext;
  takenNames: string[];
  sampleName: string;
  check?: CheckAvailabilityFn;
  mint?: MintFn;
  approve?: ApproveFn;
  track?: TrackFn;
  banner?: ReactNode;
  creditsNote?: string;
  onReturnToPublish?: (worldName: string) => void;
};

function ClaimNameWizardInner({
  stateId,
  trackCtx,
  takenNames,
  sampleName,
  check,
  mint,
  approve,
  track,
  banner,
  creditsNote,
  onReturnToPublish,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const auth = useAuth();

  const [draft, setDraft] = useState(sampleName);

  const snapshot = useRef(
    resolveClaimSnapshot({
      step: stateId,
      trackCtx,
      takenNames,
      check,
      mint,
      approve,
      track,
      name: sampleName,
    }),
  ).current;

  const [state, send] = useMachine(claimNameMachine, {
    input: { trackCtx, takenNames, check, mint, approve, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const activeName = state.context.name || draft || sampleName;

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        if (state.context.name) params.set("name", state.context.name);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, state.context.name, setSearchParams]);

  return (
    <MkClaimNameWizardView
      value={value}
      step={step}
      activeName={activeName}
      initialName={draft || sampleName}
      priceMana={state.context.priceMana}
      live={!!approve}
      error={state.context.error}
      banner={banner}
      creditsNote={creditsNote}
      onClaim={(name: string) => {
        setDraft(name);
        if (!auth.isConnected) {
          setSearchParams(
            (prev) => {
              const params = new URLSearchParams(prev);
              params.set("name", name.trim());
              return params;
            },
            { replace: true, preventScrollReset: true },
          );
          trackEvent("mk_claim_signin_gated", { name: name.trim() }, trackCtx);
          openSignIn();
          return;
        }
        send({ type: "SUBMIT_NAME", name: name.trim() });
      }}
      onBack={() => send({ type: "BACK" })}
      onApproveMana={() => send({ type: "APPROVE_MANA" })}
      onConfirmMint={() => send({ type: "CONFIRM_MINT" })}
      onRetry={() => send({ type: "RETRY" })}
      onReturnToPublish={onReturnToPublish}
    />
  );
}
