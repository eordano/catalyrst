import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import LdRsvpWizardView from "@ui/landings/workflows/LdRsvpWizardView";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  rsvpMachine,
  resolveRsvpSnapshot,
  slugToState,
  stateToSlug,
  type CommitFn,
  type TrackFn,
} from "./machine";

export type RsvpEventView = {
  id: string;
  title: string;
  when: string;
  host: string;
  description: string;
  schedule: string;
  location: string;
  jumpHref: string;
};

export type RsvpWizardProps = {
  trackCtx: TrackContext;
  event: RsvpEventView;
  count?: number;
  initialStep?: string;
  commit?: CommitFn;
  track?: TrackFn;
};

export default function RsvpWizard({
  trackCtx,
  event,
  count = 0,
  initialStep,
  commit,
  track,
}: RsvpWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <RsvpWizardInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      event={event}
      count={count}
      commit={commit}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  trackCtx: TrackContext;
  event: RsvpEventView;
  count: number;
  commit?: CommitFn;
  track?: TrackFn;
};

function RsvpWizardInner({
  stateId,
  trackCtx,
  event,
  count,
  commit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveRsvpSnapshot({
      step: stateId,
      trackCtx,
      eventId: event.id,
      count,
      commit,
      track,
    }),
  ).current;

  const [state, send] = useMachine(rsvpMachine, {
    input: { trackCtx, eventId: event.id, count, commit, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const liveCount = state.context.count;

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
    <LdRsvpWizardView
      step={step}
      value={value}
      event={event}
      count={liveCount}
      error={state.context.error}
      onTapGoing={() => send({ type: "TAP_GOING" })}
      onCancel={() => send({ type: "CANCEL" })}
      onSignIn={() => send({ type: "SIGN_IN" })}
      onBack={() => send({ type: "BACK" })}
      onConfirm={() => send({ type: "CONFIRM" })}
      onCancelRsvp={() => send({ type: "CANCEL_RSVP" })}
      onDismiss={() => send({ type: "DISMISS" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
