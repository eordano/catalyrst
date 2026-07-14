import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useNavigate, useSearchParams } from "react-router";

import GvSubmitCatalystView from "@ui/governance/workflows/GvSubmitCatalystView";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  simulateDomainStatus,
  type CatalystRequest,
  type SubmitCatalystData,
} from "@data/lib/catalyst/governance/submit-catalyst";
import {
  submitCatalystMachine,
  resolveCatalystSnapshot,
  slugToState,
  stateToSlug,
  type CreateFn,
  type TrackFn,
} from "./machine";

export type SubmitCatalystWizardProps = {
  trackCtx: TrackContext;
  request: CatalystRequest;
  data: SubmitCatalystData;
  initialStep?: string;
  create?: CreateFn;
  track?: TrackFn;
};

export default function SubmitCatalystWizard({
  trackCtx,
  request,
  data,
  initialStep,
  create,
  track,
}: SubmitCatalystWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <SubmitCatalystWizardInner
      key={stateId}
      stateId={stateId}
      request={request}
      data={data}
      trackCtx={trackCtx}
      create={create}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  request: CatalystRequest;
  data: SubmitCatalystData;
  trackCtx: TrackContext;
  create?: CreateFn;
  track?: TrackFn;
};

function SubmitCatalystWizardInner({
  stateId,
  request,
  data,
  trackCtx,
  create,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  const snapshot = useRef(
    resolveCatalystSnapshot({ step: stateId, request, trackCtx, create, track }),
  ).current;

  const [state, send] = useMachine(submitCatalystMachine, {
    input: { request, trackCtx, create, track },
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
        params.set("request", request);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, request, setSearchParams]);

  const sample = data.sample_nodes[0];

  return (
    <GvSubmitCatalystView
      value={value}
      step={step}
      request={request}
      copy={data.requests[request]}
      descriptionField={data.fields.description}
      success={data.success}
      resultId={state.context.result?.id}
      onTab={(id) => navigate(id === "home" ? "/governance" : `/governance/${id}`)}
      onSimulateInvalidDomain={() => send({ type: "DOMAIN_INVALID" })}
      onContinueDetails={() => {
        const owner = sample?.owner ?? "0x06012c8cf97bead5deae237070f9587f8e7a266d";
        const domain = sample?.domain ?? "peer.example.com";
        const status = simulateDomainStatus(domain);
        send({
          type: "FILL_DETAILS",
          owner,
          domain,
          alreadyACatalyst: status.alreadyACatalyst,
        });
      }}
      onContinueDescription={() =>
        send({
          type: "FILL_DESCRIPTION",
          description:
            request === "add"
              ? "This node expands Catalyst coverage in an under-served region, improving latency for nearby users."
              : "This node has been unreachable for an extended period and should be removed from the network.",
          coAuthors: [],
        })
      }
      onSubmit={() => send({ type: "SUBMIT" })}
      onBack={() => send({ type: "BACK" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
