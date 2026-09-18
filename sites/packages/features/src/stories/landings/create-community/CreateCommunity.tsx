import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import CommunityCreate from "@ui/explorer/components/CommunityCreate";
import "@ui/explorer/components/communitycreate.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import { validateStep, type CommunityDraft, type OwnedPlace } from "@data/lib/catalyst/overlay/create-community";
import { communityMachine, resolveCommunitySnapshot, slugToState, stateToSlug, type CommunityStateId, type CreateFn, type TrackFn } from "./machine";

type CreateCommunityProps = {
  trackCtx: TrackContext;
  ownedPlaces: OwnedPlace[];
  draft?: CommunityDraft;
  initialStep?: string;
  create?: CreateFn;
  track?: TrackFn;
};

export default function CreateCommunity({
  trackCtx,
  ownedPlaces,
  draft,
  initialStep,
  create,
  track,
}: CreateCommunityProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <CreateCommunityInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      ownedPlaces={ownedPlaces}
      draft={draft}
      create={create}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: CommunityStateId;
  trackCtx: TrackContext;
  ownedPlaces: OwnedPlace[];
  draft?: CommunityDraft;
  create?: CreateFn;
  track?: TrackFn;
};

function CreateCommunityInner({
  stateId,
  trackCtx,
  draft,
  create,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveCommunitySnapshot({ step: stateId, trackCtx, draft, create, track }),
  ).current;

  const [state] = useMachine(communityMachine, {
    input: { trackCtx, draft, create, track },
    snapshot,
  });

  const value = state.value as CommunityStateId;
  const step = stateToSlug(value);
  const d = state.context.draft;
  validateStep(value, d);

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
    <div className="create-community">
      <CommunityCreate />
    </div>
  );
}

