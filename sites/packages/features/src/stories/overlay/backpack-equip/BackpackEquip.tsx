import { useEffect, useMemo, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import BackpackEquipView from "@ui/overlay/workflows/BackpackEquipView";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  byCategory,
  equipInto,
  type Category,
  type CatalogState,
  type Equipped,
  type InventoryState,
  type Wearable,
} from "@data/lib/catalyst/overlay/backpack";
import {
  backpackMachine,
  resolveBackpackSnapshot,
  slugToState,
  stateToSlug,
  type SaveFn,
  type TrackFn,
} from "./machine";

export type BackpackEquipProps = {
  trackCtx: TrackContext;
  catalog: Wearable[];
  categories: Category[];
  equipped: Equipped;
  inventory: InventoryState;
  catalogState: CatalogState;
  initialStep?: string;
  canRetry?: boolean;
  save?: SaveFn;
  track?: TrackFn;
};

export default function BackpackEquip(props: BackpackEquipProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  const machineStep = useRef<string | null>(null);
  const seed = useRef({ key: 0, stateId });
  if (seed.current.stateId !== stateId) {
    if (machineStep.current !== stateToSlug(stateId)) seed.current.key++;
    seed.current.stateId = stateId;
  }

  return (
    <BackpackEquipInner
      key={seed.current.key}
      stateId={stateId}
      machineStepRef={machineStep}
      {...props}
    />
  );
}

type InnerProps = BackpackEquipProps & {
  stateId: ReturnType<typeof slugToState>;
  machineStepRef: { current: string | null };
};

function BackpackEquipInner({
  stateId,
  machineStepRef,
  trackCtx,
  catalog,
  categories,
  equipped,
  inventory,
  catalogState,
  canRetry = true,
  save,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const ownedEmpty = inventory.status === "loaded" && inventory.empty;

  const seedSelected = useMemo(() => {
    const w = catalog[0];
    return w ? { urn: w.urn, category: w.category, rarity: w.rarity } : undefined;
  }, [catalog]);

  const seedWearables = useMemo(() => {
    if (stateId === "equipping" || stateId === "coloring" || stateId === "reviewing" || stateId === "saving") {
      const w = catalog[0];
      return w ? equipInto(equipped.wearables, w, catalog) : equipped.wearables;
    }
    return equipped.wearables;
  }, [stateId, catalog, equipped.wearables]);

  const snapshot = useRef(
    resolveBackpackSnapshot({
      step: stateId,
      trackCtx,
      baseWearables: seedWearables,
      baseColors: {
        skin: equipped.skinColor,
        hair: equipped.hairColor,
        eye: equipped.eyeColor,
      },
      ownedEmpty,
      save,
      track,
      selected: seedSelected,
    }),
  ).current;

  const [state, send] = useMachine(backpackMachine, {
    input: {
      trackCtx,
      baseWearables: equipped.wearables,
      baseColors: {
        skin: equipped.skinColor,
        hair: equipped.hairColor,
        eye: equipped.eyeColor,
      },
      ownedEmpty,
      save,
      track,
    },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    machineStepRef.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("panel", "backpack");
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams, machineStepRef]);

  const ctx = state.context;
  const grouped = useMemo(() => byCategory(catalog), [catalog]);

  const activeCat = ctx.selectedCategory ?? "all";

  function onEquip(w: Wearable) {
    const next = equipInto(ctx.wearables, w, catalog);
    send({ type: "EQUIP", urn: w.urn, slot: w.category, wearables: next });
  }

  return (
    <BackpackEquipView
      value={value}
      step={step}
      catalog={catalog}
      categories={categories}
      activeCat={activeCat}
      selectedUrn={ctx.selectedUrn}
      bodyShape={equipped.bodyShape}
      wearables={ctx.wearables}
      baseWearables={ctx.baseWearables}
      colors={ctx.colors}
      inventory={inventory}
      catalogState={catalogState}
      error={ctx.error}
      result={ctx.result}
      onMarketplace={() => {
        if (typeof window !== "undefined")
          window.open("https://catalyst.example.com/shop", "_blank", "noopener");
      }}
      onPickCategory={(id) => {
        const first = (grouped[id] ?? [])[0];
        if (first) {
          send({
            type: "SELECT",
            urn: first.urn,
            category: first.category,
            rarity: first.rarity,
          });
        }
      }}
      onInventoryEmpty={() => send({ type: "INVENTORY_EMPTY" })}
      onSelect={(w) =>
        send({ type: "SELECT", urn: w.urn, category: w.category, rarity: w.rarity })
      }
      onEquip={onEquip}
      onAdjustColors={() => send({ type: "PICK_COLOR", kind: "skin", color: ctx.colors.skin })}
      onPickColor={(kind, color) => send({ type: "PICK_COLOR", kind, color })}
      onReview={() => send({ type: "REVIEW" })}
      onBack={() => send({ type: "BACK" })}
      onSave={() => send({ type: "SAVE" })}
      onRetry={canRetry ? () => send({ type: "RETRY" }) : undefined}
    />
  );
}
