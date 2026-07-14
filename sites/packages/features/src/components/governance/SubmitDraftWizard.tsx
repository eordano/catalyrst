import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useNavigate, useSearchParams } from "react-router";

import GvSubmitDraftView from "@ui/governance/workflows/GvSubmitDraftView";

import type { TrackContext } from "@core/lib/telemetry/track";
import type { DraftSubmitData } from "@data/lib/catalyst/governance/submit-draft";
import {
  draftMachine,
  resolveDraftSnapshot,
  slugToState,
  stateToSlug,
  type SubmitFn,
  type TrackFn,
} from "../../stories/governance/submit-draft/machine";

export type SubmitDraftWizardProps = {
  trackCtx: TrackContext;
  data: DraftSubmitData;
  submitDraft?: SubmitFn;
  track?: TrackFn;
};

export default function SubmitDraftWizard(props: SubmitDraftWizardProps) {
  const [searchParams] = useSearchParams();
  const urlStep = searchParams.get("step")?.trim() || undefined;
  const stateId = slugToState(urlStep);
  return <SubmitDraftWizardInner key={stateId} stateId={stateId} {...props} />;
}

type InnerProps = SubmitDraftWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function SubmitDraftWizardInner({
  stateId,
  trackCtx,
  data,
  submitDraft,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const { linkedPolls, vpThreshold, vpUnit, account, limits, source } = data;

  const snapshot = useRef(
    resolveDraftSnapshot({ step: stateId, trackCtx, submitDraft, track }),
  ).current;

  const [state, send] = useMachine(draftMachine, {
    input: { trackCtx, submitDraft, track },
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
    <GvSubmitDraftView
      value={value}
      step={step}
      source={source}
      linkedPolls={linkedPolls}
      vpThreshold={vpThreshold}
      vpUnit={vpUnit}
      account={account}
      limits={limits}
      draft={state.context.draft}
      error={state.context.error}
      resultProposalId={state.context.result?.proposalId}
      onTab={(id) => navigate(id === "home" ? "/governance" : `/governance/${id}`)}
      onStartDraft={(pollId) => send({ type: "CLEAR_GATE", pollId })}
      onSubmitDetails={({ title, bodies }) =>
        send({ type: "SUBMIT_DETAILS", title, bodies })
      }
      onContinueCoauthors={(coauthors) => send({ type: "NEXT", coauthors })}
      onSubmit={() => send({ type: "SUBMIT" })}
      onBack={() => send({ type: "BACK" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
