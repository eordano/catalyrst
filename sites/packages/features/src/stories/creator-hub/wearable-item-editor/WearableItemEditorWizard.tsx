import { useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useNavigate, useSearchParams } from "react-router";

import WearableItemEditorView from "@ui/creatorhub/workflows/WearableItemEditorView";

import { maxSupplyFor } from "@data/lib/catalyst/builder/item-editor";
import type { TrackContext } from "@core/lib/telemetry/track";
import {
  wearableEditorMachine,
  resolveWearableEditorSnapshot,
  slugToState,
  stateToSlug,
  type WearableDraft,
  type SaveFn,
  type TrackFn,
} from "./machine";

export type CollectionOption = {
  id: string;
  name: string;
  items: { id: string; name: string; type: "wearable" | "emote" }[];
  itemsLoaded?: boolean;
  itemCount?: number;
  status?: string;
};

export type WearableItemEditorWizardProps = {
  trackCtx: TrackContext;
  draft: WearableDraft;
  collections: CollectionOption[];
  categories: readonly string[];
  rarities: readonly string[];
  initialStep?: string;
  save?: SaveFn;
  track?: TrackFn;
};

function newDraftItemId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `new-${Date.now()}`;
}

export default function WearableItemEditorWizard(props: WearableItemEditorWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = downgradeTransient(slugToState(urlStep));

  return <WearableItemEditorWizardInner stateId={stateId} {...props} />;
}

function downgradeTransient(
  state: ReturnType<typeof slugToState>,
): ReturnType<typeof slugToState> {
  if (state === "saving") return "price";
  if (state === "saved") return "selecting";
  return state;
}

type InnerProps = WearableItemEditorWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function WearableItemEditorWizardInner({
  stateId,
  trackCtx,
  draft,
  collections,
  categories,
  rarities,
  save,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  const snapshot = useRef(
    resolveWearableEditorSnapshot({ step: stateId, trackCtx, draft, save, track }),
  ).current;

  const [state, send] = useMachine(wearableEditorMachine, {
    input: { trackCtx, draft, save, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const d = state.context.draft;

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

  const [category, setCategory] = useState(d.category);
  const [rarity, setRarity] = useState(d.rarity);
  const [price, setPrice] = useState(d.price);
  const [free, setFree] = useState(d.free);

  const selectedCollection =
    collections.find((c) => c.id === d.collectionId) ?? collections[0];

  const doneCollectionId = selectedCollection?.id ?? d.collectionId;
  const doneTarget = doneCollectionId
    ? `/create/wearables/collections/${encodeURIComponent(doneCollectionId)}`
    : "/create/wearables";

  return (
    <WearableItemEditorView
      value={value}
      step={step}
      draft={{
        name: d.name,
        rarity: d.rarity,
        price: d.price,
        free: d.free,
      }}
      collectionId={selectedCollection?.id ?? ""}
      collectionName={selectedCollection?.name ?? "Collection"}
      collectionStatus={selectedCollection?.status ?? ""}
      itemCount={
        selectedCollection
          ? selectedCollection.itemsLoaded
            ? selectedCollection.items.length
            : (selectedCollection.itemCount ?? selectedCollection.items.length)
          : 0
      }
      items={selectedCollection?.items ?? []}
      itemsLoaded={selectedCollection?.itemsLoaded ?? false}
      collections={collections.map((c) => ({
        id: c.id,
        name: c.name,
        itemCount: c.itemsLoaded ? c.items.length : c.itemCount,
        status: c.status,
      }))}
      categories={categories}
      rarities={rarities}
      category={category}
      rarity={rarity}
      price={price}
      free={free}
      maxSupply={maxSupplyFor(rarity)}
      error={state.context.error ?? ""}
      onSelectNew={() =>
        send({
          type: "SELECT_ITEM",
          collectionId: selectedCollection?.id ?? d.collectionId,
          itemId: newDraftItemId(),
          name: "New wearable",
        })
      }
      onSelectItem={(it: CollectionOption["items"][number]) =>
        send({
          type: "SELECT_ITEM",
          collectionId: selectedCollection?.id ?? d.collectionId,
          itemId: it.id,
          name: it.name,
        })
      }
      onBack={() => send({ type: "BACK" })}
      onNameChange={(name: string) => send({ type: "SET_NAME", name })}
      onSetModel={(fileName?: string) =>
        send({ type: "SET_MODEL", modelFile: fileName ?? `male/${d.itemId}.glb` })
      }
      onCategoryChange={setCategory}
      onContinueCategory={() => send({ type: "SET_CATEGORY", category })}
      onRarityChange={setRarity}
      onContinueRarity={() => send({ type: "SET_RARITY", rarity })}
      onPriceChange={setPrice}
      onFreeChange={setFree}
      onSave={() => send({ type: "SET_PRICE", price, free })}
      onRetry={() => send({ type: "RETRY" })}
      onRevert={() => send({ type: "REVERT" })}
      onAddAnother={() => send({ type: "ADD_ANOTHER" })}
      onDone={() => navigate(doneTarget)}
    />
  );
}
