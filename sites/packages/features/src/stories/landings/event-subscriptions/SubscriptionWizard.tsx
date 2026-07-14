import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import LdSubscriptionWizardView from "@ui/landings/workflows/LdSubscriptionWizardView";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  subscriptionMachine,
  resolveSubscriptionSnapshot,
  slugToState,
  stateToSlug,
  enabledTypes,
  type CommitFn,
  type Selection,
  type TrackFn,
} from "./machine";

export type SubscriptionGroup = {
  key: string;
  label: string;
  flag?: string;
  types: string[];
};

export type SubscriptionWizardProps = {
  trackCtx: TrackContext;
  email: string;
  emailConfirmed: boolean;
  selection: Selection;
  groups: SubscriptionGroup[];
  initialStep?: string;
  commit?: CommitFn;
  track?: TrackFn;
};

export default function SubscriptionWizard(props: SubscriptionWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return <SubscriptionWizardInner key={stateId} stateId={stateId} {...props} />;
}

type InnerProps = SubscriptionWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function SubscriptionWizardInner({
  stateId,
  trackCtx,
  email,
  emailConfirmed,
  selection,
  groups,
  commit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveSubscriptionSnapshot({ step: stateId, trackCtx, selection, commit, track }),
  ).current;

  const [state, send] = useMachine(subscriptionMachine, {
    input: { trackCtx, selection, commit, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const sel = state.context.selection;
  const enabledCount = enabledTypes(sel).length;

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
    <LdSubscriptionWizardView
      step={step}
      value={value}
      email={email}
      emailConfirmed={emailConfirmed}
      selection={sel}
      groups={groups}
      enabledCount={enabledCount}
      error={state.context.error}
      lastKind={state.context.lastKind}
      onStart={() => send({ type: "START" })}
      onSignIn={() => send({ type: "SIGN_IN" })}
      onToggle={(notificationType, enabled) =>
        send({ type: "TOGGLE", notificationType, enabled })
      }
      onSubmit={() => send({ type: "SUBMIT" })}
      onEdit={() => send({ type: "EDIT" })}
      onUnsubscribe={() => send({ type: "UNSUBSCRIBE" })}
      onResubscribe={() => send({ type: "RESUBSCRIBE" })}
      onRetry={() => send({ type: "RETRY" })}
      onBack={() => send({ type: "BACK" })}
    />
  );
}
