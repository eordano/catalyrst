import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { Link, useNavigate, useSearchParams } from "react-router";

import GvSubmitBidView from "@ui/governance/workflows/GvSubmitBidView";

import type { TrackContext } from "@core/lib/telemetry/track";
import type { SubmitBidData } from "@data/lib/catalyst/governance/submit-bid";
import {
  bidMachine,
  resolveBidSnapshot,
  slugToState,
  stateToSlug,
  type SubmitFn,
  type TrackFn,
} from "./machine";

export type GvSubmitBidWizardProps = {
  trackCtx: TrackContext;
  data: SubmitBidData;
  initialStep?: string;
  submitBid?: SubmitFn;
  track?: TrackFn;
};

export default function GvSubmitBidWizard(props: GvSubmitBidWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  const tenderId = props.data.parents.tender.id;

  return (
    <GvSubmitBidWizardInner key={`${stateId}:${tenderId}`} stateId={stateId} {...props} />
  );
}

type InnerProps = GvSubmitBidWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function GvSubmitBidWizardInner({ stateId, trackCtx, data, submitBid, track }: InnerProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const tenderId = data.parents.tender.id;
  const sample = data.sampleDraft;

  const buildTenderHref = (id: string) => {
    const p = new URLSearchParams(searchParams);
    p.set("linked_proposal_id", id);
    p.set("step", "parents");
    return `?${p.toString()}`;
  };

  const snapshot = useRef(
    resolveBidSnapshot({
      step: stateId,
      trackCtx,
      tenderId,
      submitBid,
      track,
      draft:
        stateId === "parents"
          ? undefined
          : { tenderId, budget: sample.funding, duration: sample.projectDuration },
    }),
  ).current;

  const [state, send] = useMachine(bidMachine, {
    input: { trackCtx, tenderId, submitBid, track },
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
    <GvSubmitBidView
      value={value}
      step={step}
      copy={data.copy}
      account={data.account}
      parents={data.parents}
      tenders={data.tenders.map((t) => ({
        id: t.id,
        title: t.title,
        href: buildTenderHref(t.id),
      }))}
      fallback={data.fallback}
      submitError={data.submit_error}
      success={data.success}
      error={state.context.error}
      resultProposalId={state.context.result?.proposalId}
      LinkComponent={Link}
      onTab={(id) => navigate(id === "home" ? "/governance" : `/governance/${id}`)}
      onContinue={() => send({ type: "CONTINUE" })}
      onSetFunding={() =>
        send({
          type: "SET_FUNDING",
          budget: sample.funding,
          duration: sample.projectDuration,
        })
      }
      onNext={() => send({ type: "NEXT" })}
      onSubmit={() => send({ type: "SUBMIT" })}
      onBack={() => send({ type: "BACK" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
