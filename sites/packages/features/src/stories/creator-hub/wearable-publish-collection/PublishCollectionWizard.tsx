import { useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useNavigate, useSearchParams } from "react-router";

import ChPublishCollectionView from "@ui/creatorhub/workflows/ChPublishCollectionView";
import type { ChCollectionItem } from "@ui/creatorhub/pages/ChCollectionDetail";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  publishMachine,
  resolvePublishSnapshot,
  slugToState,
  stateToSlug,
  type PublishCollection,
  type PublishFn,
  type TrackFn,
} from "./machine";

export type SummaryView = {
  collection: {
    name: string;
    status: string;
    isPublished: boolean;
    isApproved: boolean;
    isOnSale: boolean;
    isLocked: boolean;
  };
  wearables: ChCollectionItem[];
  emotes: ChCollectionItem[];
};

export type PublishCollectionWizardProps = {
  collection: PublishCollection;
  summary: SummaryView;
  manaPerItem?: number;
  trackCtx: TrackContext;
  initialStep?: string;
  publish?: PublishFn;
  track?: TrackFn;
};

export default function PublishCollectionWizard({
  collection,
  summary,
  manaPerItem,
  trackCtx,
  initialStep,
  publish,
  track,
}: PublishCollectionWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const raw = slugToState(urlStep);
  const stateId = raw === "pay" && collection.items.length > 0 ? "terms" : raw;

  return (
    <PublishCollectionWizardInner
      stateId={stateId}
      collection={collection}
      summary={summary}
      manaPerItem={manaPerItem}
      trackCtx={trackCtx}
      publish={publish}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  collection: PublishCollection;
  summary: SummaryView;
  manaPerItem?: number;
  trackCtx: TrackContext;
  publish?: PublishFn;
  track?: TrackFn;
};

function PublishCollectionWizardInner({
  stateId,
  collection,
  summary,
  manaPerItem,
  trackCtx,
  publish,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const [accepted, setAccepted] = useState(false);

  const snapshot = useRef(
    resolvePublishSnapshot({
      step: stateId,
      collection,
      trackCtx,
      manaPerItem,
      publish,
      track,
    }),
  ).current;

  const [state, send] = useMachine(publishMachine, {
    input: { collection, trackCtx, manaPerItem, publish, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const fee = state.context.fee;

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    if (step === "blocked") return;
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
    <ChPublishCollectionView
      step={step}
      view={value}
      collectionName={collection.name}
      summary={summary}
      fee={fee}
      txHash={state.context.result?.txHash ?? ""}
      error={state.context.error ?? ""}
      statusHref={`/create/wearables/collections/${encodeURIComponent(collection.id)}`}
      accepted={accepted}
      onAcceptedChange={setAccepted}
      onNext={() => send({ type: "NEXT" })}
      onBack={() => send({ type: "BACK" })}
      onAccept={() => send({ type: "ACCEPT" })}
      onRetry={() => send({ type: "RETRY" })}
      onDone={() => navigate("/create/wearables")}
    />
  );
}
