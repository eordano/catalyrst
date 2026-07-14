import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GvLinkAccountsView from "@ui/governance/workflows/GvLinkAccountsView";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  type LinkAccountsData,
  type Provider,
} from "@data/lib/catalyst/governance/link-accounts";
import {
  linkAccountsMachine,
  resolveLinkSnapshot,
  slugToState,
  stateToSlug,
  stepsFor,
  type VerifyFn,
  type UnlinkFn,
  type TrackFn,
  type LinkStateId,
} from "./machine";

export type LinkAccountsWizardProps = {
  trackCtx: TrackContext;
  account: Provider;
  data: LinkAccountsData;
  initialStep?: string;
  unlinkOnMount?: boolean;
  verify?: VerifyFn;
  unlink?: UnlinkFn;
  track?: TrackFn;
};

export default function LinkAccountsWizard({
  trackCtx,
  account,
  data,
  initialStep,
  unlinkOnMount,
  verify,
  unlink,
  track,
}: LinkAccountsWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const action = searchParams.get("action")?.trim();
  const effectiveStep =
    urlStep ?? ((unlinkOnMount || action === "unlink") ? "unlink" : undefined);
  const stateId = slugToState(effectiveStep);

  return (
    <LinkAccountsWizardInner
      key={`${stateId}:${account}`}
      stateId={stateId}
      account={account}
      data={data}
      trackCtx={trackCtx}
      verify={verify}
      unlink={unlink}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: LinkStateId;
  account: Provider;
  data: LinkAccountsData;
  trackCtx: TrackContext;
  verify?: VerifyFn;
  unlink?: UnlinkFn;
  track?: TrackFn;
};

function LinkAccountsWizardInner({
  stateId,
  account,
  data,
  trackCtx,
  verify,
  unlink,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveLinkSnapshot({ step: stateId, account, trackCtx, verify, unlink, track }),
  ).current;

  const [state, send] = useMachine(linkAccountsMachine, {
    input: { account, trackCtx, verify, unlink, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const ctxAccount = state.context.account;

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        params.set("account", ctxAccount);
        if (step !== "unlink") params.delete("action");
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, ctxAccount, setSearchParams]);

  return (
    <GvLinkAccountsView
      value={value}
      step={step}
      account={ctxAccount}
      connectStep={state.context.connectStep}
      totalSteps={state.context.totalSteps}
      title={data.title}
      providers={data.providers}
      accounts={data.accounts}
      unlink={data.unlink}
      onChoose={(p) => {
        const provider = p as Provider;
        send({ type: "CHOOSE", account: provider, totalSteps: stepsFor(provider) });
      }}
      onUnlinkRequest={(a) => send({ type: "UNLINK_REQUEST", account: a as Provider })}
      onNextStep={() => send({ type: "NEXT_STEP" })}
      onConfirm={() => send({ type: "CONFIRM" })}
      onCancel={() => send({ type: "CANCEL" })}
      onConfirmUnlink={() => send({ type: "CONFIRM_UNLINK" })}
      onBack={() => send({ type: "BACK" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
