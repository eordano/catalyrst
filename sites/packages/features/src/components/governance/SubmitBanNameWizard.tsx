import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useNavigate, useSearchParams } from "react-router";

import GvSubmitBanNameView from "@ui/governance/workflows/GvSubmitBanNameView";

import type { TrackContext } from "@core/lib/telemetry/track";
import type { BanNameSubmitContext } from "@data/lib/catalyst/governance/submit-ban-name";
import {
  banNameMachine,
  resolveBanNameSnapshot,
  slugToState,
  stateToSlug,
  type BanNameDraft,
  type SubmitFn,
  type TrackFn,
} from "../../stories/governance/submit-ban-name/machine";

export type SubmitBanNameWizardProps = {
  trackCtx: TrackContext;
  ctx: BanNameSubmitContext;
  initialStep?: string;
  submit?: SubmitFn;
  track?: TrackFn;
};

export default function SubmitBanNameWizard({
  trackCtx,
  ctx,
  initialStep,
  submit,
  track,
}: SubmitBanNameWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <SubmitBanNameWizardInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      ctx={ctx}
      submit={submit}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  trackCtx: TrackContext;
  ctx: BanNameSubmitContext;
  submit?: SubmitFn;
  track?: TrackFn;
};

function SubmitBanNameWizardInner({
  stateId,
  trackCtx,
  ctx,
  submit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  const seededDraft: BanNameDraft = {
    name: ctx.sample.name,
    description: ctx.sample.description,
    coAuthors: ctx.sample.coAuthors,
  };
  const snapshot = useRef(
    resolveBanNameSnapshot({
      step: stateId,
      trackCtx,
      submit,
      track,
      draft: seededDraft,
    }),
  ).current;

  const [state, send] = useMachine(banNameMachine, {
    input: { trackCtx, submit, track },
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
    <GvSubmitBanNameView
      value={value}
      step={step}
      ctx={ctx}
      draft={state.context.draft}
      errors={state.context.errors}
      error={state.context.error}
      resultProposalId={state.context.result?.proposalId}
      onTab={(id) => navigate(id === "home" ? "/governance" : `/governance/${id}`)}
      onSubmitName={(name) => send({ type: "SUBMIT_NAME", name })}
      onSubmitDescription={({ description, coAuthors }) =>
        send({ type: "SUBMIT_DESCRIPTION", description, coAuthors })
      }
      onConfirm={() => send({ type: "CONFIRM" })}
      onBack={() => send({ type: "BACK" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
