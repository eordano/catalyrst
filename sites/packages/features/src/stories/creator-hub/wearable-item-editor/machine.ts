import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { maxSupplyFor } from "@data/lib/catalyst/builder/item-editor";
import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type WearableDraft = {
  collectionId: string;
  itemId: string;
  name: string;
  modelFile: string;
  category: string;
  rarity: string;
  price: string;
  free: boolean;
};

export type SaveResult = { itemId: string; urn: string };

export type SaveFn = (args: {
  draft: WearableDraft;
  signal?: AbortSignal;
}) => Promise<SaveResult>;

export type WearableEditorInput = {
  trackCtx: TrackContext;
  draft: WearableDraft;
  save?: SaveFn;
  track?: TrackFn;
};

export type WearableEditorContext = {
  trackCtx: TrackContext;
  baseline: WearableDraft;
  draft: WearableDraft;
  save: SaveFn;
  track: TrackFn;
  result?: SaveResult;
  error?: string;
};

export type WearableEditorEvent =
  | { type: "SELECT_ITEM"; collectionId: string; itemId: string; name: string }
  | { type: "SET_NAME"; name: string }
  | { type: "SET_MODEL"; modelFile: string }
  | { type: "SET_CATEGORY"; category: string }
  | { type: "SET_RARITY"; rarity: string }
  | { type: "SET_PRICE"; price: string; free: boolean }
  | { type: "BACK" }
  | { type: "REVERT" }
  | { type: "RETRY" }
  | { type: "ADD_ANOTHER" };

export const WEARABLE_EDITOR_EVENTS = {
  opened: "bd_item_editor_opened",
  modelSet: "bd_item_model_set",
  categorySet: "bd_item_category_set",
  raritySet: "bd_item_rarity_set",
  priceSet: "bd_item_price_set",
  saved: "bd_item_saved",
  reverted: "bd_item_reverted",
} as const;

export const STATE_TO_SLUG = {
  selecting: "select",
  model: "model",
  category: "category",
  rarity: "rarity",
  price: "price",
  saving: "save",
  saved: "saved",
  error: "error",
} as const;

export type WearableEditorStateId = keyof typeof STATE_TO_SLUG;
export type WearableEditorStepSlug = (typeof STATE_TO_SLUG)[WearableEditorStateId];

export const FIRST_STEP_SLUG: WearableEditorStepSlug = STATE_TO_SLUG.selecting;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "selecting");

export const SLUG_TO_STATE: Record<WearableEditorStepSlug, WearableEditorStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => WearableEditorStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => WearableEditorStateId = stepSlugs.toState;

export const simulateSave: SaveFn = async ({ draft, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return {
    itemId: draft.itemId,
    urn: `urn:decentraland:matic:collections-v2:0x${draft.collectionId}:${draft.itemId}`,
  };
};

export const wearableEditorMachine = setup({
  types: {
    context: {} as WearableEditorContext,
    events: {} as WearableEditorEvent,
    input: {} as WearableEditorInput,
  },
  actors: {
    runSave: fromPromise<SaveResult, { draft: WearableDraft; save: SaveFn }>(
      ({ input, signal }) => input.save({ draft: input.draft, signal }),
    ),
  },
  actions: {
    selectItem: assign(({ context, event }) => {
      if (event.type !== "SELECT_ITEM") return {};
      const draft: WearableDraft = {
        ...context.draft,
        collectionId: event.collectionId,
        itemId: event.itemId,
        name: event.name,
      };
      return { draft, baseline: draft };
    }),
    trackOpened: ({ context, event }) => {
      if (event.type !== "SELECT_ITEM") return;
      context.track(
        WEARABLE_EDITOR_EVENTS.opened,
        { item: event.itemId, collection: event.collectionId },
        context.trackCtx,
      );
    },
    setName: assign({
      draft: ({ context, event }) =>
        event.type === "SET_NAME"
          ? { ...context.draft, name: event.name }
          : context.draft,
    }),
    setModel: assign({
      draft: ({ context, event }) =>
        event.type === "SET_MODEL"
          ? { ...context.draft, modelFile: event.modelFile }
          : context.draft,
    }),
    trackModelSet: ({ context }) =>
      context.track(
        WEARABLE_EDITOR_EVENTS.modelSet,
        { item: context.draft.itemId, model: context.draft.modelFile },
        context.trackCtx,
      ),
    setCategory: assign({
      draft: ({ context, event }) =>
        event.type === "SET_CATEGORY"
          ? { ...context.draft, category: event.category }
          : context.draft,
    }),
    trackCategorySet: ({ context }) =>
      context.track(
        WEARABLE_EDITOR_EVENTS.categorySet,
        { item: context.draft.itemId, category: context.draft.category },
        context.trackCtx,
      ),
    setRarity: assign({
      draft: ({ context, event }) =>
        event.type === "SET_RARITY"
          ? { ...context.draft, rarity: event.rarity }
          : context.draft,
    }),
    trackRaritySet: ({ context }) =>
      context.track(
        WEARABLE_EDITOR_EVENTS.raritySet,
        {
          item: context.draft.itemId,
          rarity: context.draft.rarity,
          max_supply: maxSupplyFor(context.draft.rarity),
        },
        context.trackCtx,
      ),
    setPrice: assign({
      draft: ({ context, event }) =>
        event.type === "SET_PRICE"
          ? {
              ...context.draft,
              price: event.free ? "" : event.price,
              free: event.free,
            }
          : context.draft,
    }),
    trackPriceSet: ({ context }) =>
      context.track(
        WEARABLE_EDITOR_EVENTS.priceSet,
        {
          item: context.draft.itemId,
          price: context.draft.free ? "free" : context.draft.price,
          free: context.draft.free,
        },
        context.trackCtx,
      ),
    commitSave: assign(({ context, event }) => {
      const result = (event as { output?: SaveResult }).output;
      return { result, baseline: context.draft };
    }),
    trackSaved: ({ context }) =>
      context.track(
        WEARABLE_EDITOR_EVENTS.saved,
        {
          item: context.draft.itemId,
          rarity: context.draft.rarity,
          price: context.draft.free ? "free" : context.draft.price,
          urn: context.result?.urn,
          stub: true,
        },
        context.trackCtx,
      ),
    revertDraft: assign({
      draft: ({ context }) => context.baseline,
      error: undefined,
      result: undefined,
    }),
    trackReverted: ({ context }) =>
      context.track(
        WEARABLE_EDITOR_EVENTS.reverted,
        { item: context.draft.itemId },
        context.trackCtx,
      ),
    recordError: assign({
      error: ({ event }) => {
        const e = (event as { error?: unknown }).error;
        return toErrorMessage(e, "save failed");
      },
    }),
  },
}).createMachine({
  id: "wearableItemEditor",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    baseline: input.draft,
    draft: input.draft,
    save: input.save ?? simulateSave,
    track: input.track ?? defaultTrack,
  }),
  initial: "selecting",
  states: {
    selecting: {
      on: {
        SELECT_ITEM: {
          target: "model",
          actions: ["selectItem", "trackOpened"],
        },
      },
    },
    model: {
      on: {
        SET_NAME: { actions: "setName" },
        SET_MODEL: { target: "category", actions: ["setModel", "trackModelSet"] },
        BACK: { target: "selecting" },
        REVERT: { target: "selecting", actions: ["revertDraft", "trackReverted"] },
      },
    },
    category: {
      on: {
        SET_CATEGORY: { target: "rarity", actions: ["setCategory", "trackCategorySet"] },
        BACK: { target: "model" },
        REVERT: { target: "selecting", actions: ["revertDraft", "trackReverted"] },
      },
    },
    rarity: {
      on: {
        SET_RARITY: { target: "price", actions: ["setRarity", "trackRaritySet"] },
        BACK: { target: "category" },
        REVERT: { target: "selecting", actions: ["revertDraft", "trackReverted"] },
      },
    },
    price: {
      on: {
        SET_PRICE: { target: "saving", actions: ["setPrice", "trackPriceSet"] },
        BACK: { target: "rarity" },
        REVERT: { target: "selecting", actions: ["revertDraft", "trackReverted"] },
      },
    },
    saving: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSave",
        src: "runSave",
        input: ({ context }) => ({ draft: context.draft, save: context.save }),
        onDone: { target: "saved", actions: ["commitSave", "trackSaved"] },
        onError: { target: "error", actions: "recordError" },
      },
    },
    saved: {
      on: {
        ADD_ANOTHER: { target: "selecting", actions: "revertDraft" },
      },
    },
    error: {
      on: {
        RETRY: { target: "saving" },
        REVERT: { target: "selecting", actions: ["revertDraft", "trackReverted"] },
      },
    },
  },
});

export type WearableEditorMachine = typeof wearableEditorMachine;

export function resolveWearableEditorSnapshot(args: {
  step: WearableEditorStateId;
  trackCtx: TrackContext;
  draft: WearableDraft;
  save?: SaveFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, draft, save, track } = args;
  if (step === "selecting") return undefined;
  const context: WearableEditorContext = {
    trackCtx,
    baseline: draft,
    draft,
    save: save ?? simulateSave,
    track: track ?? defaultTrack,
  };
  return wearableEditorMachine.resolveState({ value: step, context });
}
