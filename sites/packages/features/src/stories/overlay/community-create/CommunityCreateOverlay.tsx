import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import CommunityCreateView from "@ui/overlay/panels/CommunityCreateView";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  COMMUNITY_BOUNDS,
  validateStep,
  type CommunityDraft,
  type CommunityPrivacy,
} from "@data/lib/catalyst/overlay/community-create";
import {
  communityMachine,
  emitStepCompleted,
  FORM_ORDER,
  resolveCommunitySnapshot,
  slugToState,
  stateToSlug,
  STATE_TO_SLUG,
  type CommunityStateId,
  type CreateFn,
  type TrackFn,
} from "./machine";

export type CommunityCreateOverlayProps = {
  trackCtx: TrackContext;
  hasName: boolean;
  membershipOptions: { value: CommunityPrivacy; label: string; note: string }[];
  draft?: CommunityDraft;
  initialStep?: string;
  create?: CreateFn;
  track?: TrackFn;
};

export default function CommunityCreateOverlay({
  trackCtx,
  hasName,
  membershipOptions,
  draft,
  initialStep,
  create,
  track,
}: CommunityCreateOverlayProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <CommunityCreateInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      hasName={hasName}
      membershipOptions={membershipOptions}
      draft={draft}
      create={create}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: CommunityStateId;
  trackCtx: TrackContext;
  hasName: boolean;
  membershipOptions: { value: CommunityPrivacy; label: string; note: string }[];
  draft?: CommunityDraft;
  create?: CreateFn;
  track?: TrackFn;
};

function CommunityCreateInner({
  stateId,
  trackCtx,
  hasName,
  membershipOptions,
  draft,
  create,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveCommunitySnapshot({ step: stateId, trackCtx, hasName, draft, create, track }),
  ).current;

  const [state, send] = useMachine(communityMachine, {
    input: { trackCtx, hasName, draft, create, track },
    snapshot,
  });

  const value = state.value as CommunityStateId;
  const step = stateToSlug(value);
  const d = state.context.draft;
  const issues = validateStep(value, d);
  const trackFn = track ?? state.context.track;

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (!params.get("panel")) params.set("panel", "communities");
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams]);

  return (
    <CommunityCreateView
      value={value}
      step={step}
      hasName={hasName}
      draft={d}
      issues={issues}
      error={state.context.error}
      resultId={state.context.result?.id}
      membershipOptions={membershipOptions}
      crumbLabels={STATE_TO_SLUG}
      nameMax={COMMUNITY_BOUNDS.nameMax}
      onOpen={() => send({ type: "OPEN" })}
      onGetName={() => send({ type: "GET_NAME" })}
      onBack={() => send({ type: "BACK" })}
      onNext={() => advance(send, state, trackFn, trackCtx)}
      onEdit={(patch) => send({ type: "EDIT", patch })}
      onSubmit={() => send({ type: "SUBMIT" })}
    />
  );
}

function advance(
  send: (e: { type: "NEXT" }) => void,
  state: { value: unknown; context: { draft: CommunityDraft } },
  track: TrackFn,
  ctx: TrackContext,
): void {
  const from = state.value as CommunityStateId;
  const idx = FORM_ORDER.indexOf(from);
  const to = idx >= 0 && idx + 1 < FORM_ORDER.length ? FORM_ORDER[idx + 1] : undefined;
  const willAdvance =
    to !== undefined &&
    Object.keys(validateStep(from, state.context.draft)).length === 0;
  send({ type: "NEXT" });
  if (willAdvance && to) emitStepCompleted(track, ctx, from, to);
}
