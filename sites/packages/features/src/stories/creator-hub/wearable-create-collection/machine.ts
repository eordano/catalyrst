import { makeStepSlugs, slugify } from "@core/lib/stories/index";
import { toErrorMessage } from "@core/lib/errors";
import { assign, fromPromise, setup } from "xstate";

import { saveSimCollectionItems } from "@data/lib/catalyst/builder/sim-collection-items";
import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type CollectionType = "standard" | "linked";

export type DraftItem = {
  id: string;
  name: string;
  size: number;
  fileType: string;
  thumbnail?: string;
};

export type { TrackFn };

export type MintResult = { collectionId: string; contractAddress: string };

export type MintFn = (args: {
  name: string;
  type: CollectionType;
  items: DraftItem[];
  signal?: AbortSignal;
}) => Promise<MintResult>;

export type CreateCollectionInput = {
  trackCtx: TrackContext;
  type?: CollectionType;
  feePerItem?: number;
  mint?: MintFn;
  track?: TrackFn;
};

export type CreateCollectionContext = {
  trackCtx: TrackContext;
  feePerItem: number;
  mint: MintFn;
  track: TrackFn;
  name: string;
  type: CollectionType;
  items: DraftItem[];
  result?: MintResult;
  error?: string;
};

export type CreateCollectionEvent =
  | { type: "SUBMIT_NAME"; name: string }
  | { type: "ADD_ITEMS"; items: DraftItem[] }
  | { type: "SUBMIT" }
  | { type: "BACK" }
  | { type: "RETRY" }
  | { type: "GOTO"; step: CreateStateId };

export const CREATE_COLLECTION_EVENTS = {
  started: "bd_create_collection_started",
  named: "bd_create_collection_named",
  itemsAdded: "bd_create_collection_items_added",
  reviewReached: "bd_create_collection_review_reached",
  submitted: "bd_create_collection_submitted",
  completed: "bd_create_collection_completed",
} as const;

export const DEFAULT_FEE_PER_ITEM = 100;

export const ACCEPTED_FILE_EXTENSIONS = [".zip", ".gltf", ".glb", ".png"] as const;

export const NAME_MAX = 32;
export function isValidName(name: string): boolean {
  const t = name.trim();
  return t.length >= 1 && t.length <= NAME_MAX;
}

export function parseCollectionType(raw: string | null | undefined): CollectionType {
  const v = raw?.trim().toLowerCase();
  return v === "linked" || v === "third_party" || v === "third-party"
    ? "linked"
    : "standard";
}

export function publishCost(
  type: CollectionType,
  itemCount: number,
  feePerItem: number,
): number {
  if (type === "linked") return 0;
  return Math.max(0, itemCount) * feePerItem;
}

export const STATE_TO_SLUG = {
  naming: "name",
  editingItems: "items",
  reviewing: "review",
  submitting: "submit",
  done: "done",
  error: "error",
} as const;

export type CreateStateId = keyof typeof STATE_TO_SLUG;
export type CreateStepSlug = (typeof STATE_TO_SLUG)[CreateStateId];

export const FIRST_STEP_SLUG: CreateStepSlug = STATE_TO_SLUG.naming;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "naming");

export const SLUG_TO_STATE: Record<CreateStepSlug, CreateStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => CreateStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => CreateStateId = stepSlugs.toState;

export const simulateMint: MintFn = async ({ name, items, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const slug = slugify(name);
  const collectionId = `sim-${slug || "collection"}`;
  saveSimCollectionItems(collectionId, items);
  const contractAddress = "0x0000000000000000000000000000000000000000";
  return { collectionId, contractAddress };
};

export const createCollectionMachine = setup({
  types: {
    context: {} as CreateCollectionContext,
    events: {} as CreateCollectionEvent,
    input: {} as CreateCollectionInput,
  },
  actors: {
    runMint: fromPromise<
      MintResult,
      { name: string; type: CollectionType; items: DraftItem[]; mint: MintFn }
    >(({ input, signal }) =>
      input.mint({ name: input.name, type: input.type, items: input.items, signal }),
    ),
  },
  actions: {
    setName: assign({
      name: ({ event }) => (event.type === "SUBMIT_NAME" ? event.name.trim() : ""),
    }),
    trackStarted: ({ context, event }) => {
      if (event.type !== "SUBMIT_NAME") return;
      context.track(
        CREATE_COLLECTION_EVENTS.started,
        { type: context.type },
        context.trackCtx,
      );
      context.track(
        CREATE_COLLECTION_EVENTS.named,
        { name: event.name.trim() },
        context.trackCtx,
      );
    },
    setItems: assign({
      items: ({ event }) => (event.type === "ADD_ITEMS" ? event.items : []),
    }),
    trackItemsAdded: ({ context }) =>
      context.track(
        CREATE_COLLECTION_EVENTS.itemsAdded,
        { count: context.items.length },
        context.trackCtx,
      ),
    trackReviewReached: ({ context }) =>
      context.track(
        CREATE_COLLECTION_EVENTS.reviewReached,
        {
          count: context.items.length,
          type: context.type,
          cost_mana: publishCost(context.type, context.items.length, context.feePerItem),
        },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        CREATE_COLLECTION_EVENTS.submitted,
        {
          type: context.type,
          count: context.items.length,
          cost_mana: publishCost(context.type, context.items.length, context.feePerItem),
        },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        CREATE_COLLECTION_EVENTS.completed,
        {
          collection_id: context.result?.collectionId,
          contract_address: context.result?.contractAddress,
          type: context.type,
          count: context.items.length,
          stub: true,
        },
        context.trackCtx,
      ),
  },
  guards: {
    nameValid: ({ event }) =>
      event.type === "SUBMIT_NAME" && isValidName(event.name),
    hasItems: ({ event }) =>
      event.type === "ADD_ITEMS" && event.items.length >= 1,
    canSubmit: ({ context }) =>
      context.items.length >= 1 && isValidName(context.name),
    gotoNaming: ({ event }) => event.type === "GOTO" && event.step === "naming",
    gotoEditingItems: ({ context, event }) =>
      event.type === "GOTO" &&
      event.step === "editingItems" &&
      isValidName(context.name),
    gotoReviewing: ({ context, event }) =>
      event.type === "GOTO" &&
      event.step === "reviewing" &&
      context.items.length >= 1 &&
      isValidName(context.name),
  },
}).createMachine({
  id: "createWearableCollectionWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    feePerItem: input.feePerItem ?? DEFAULT_FEE_PER_ITEM,
    mint: input.mint ?? simulateMint,
    track: input.track ?? defaultTrack,
    name: "",
    type: input.type ?? "standard",
    items: [],
  }),
  initial: "naming",
  states: {
    naming: {
      on: {
        SUBMIT_NAME: {
          guard: "nameValid",
          target: "editingItems",
          actions: ["setName", "trackStarted"],
        },
        GOTO: [
          { guard: "gotoEditingItems", target: "editingItems" },
          { guard: "gotoReviewing", target: "reviewing" },
        ],
      },
    },
    editingItems: {
      on: {
        ADD_ITEMS: {
          guard: "hasItems",
          target: "reviewing",
          actions: ["setItems", "trackItemsAdded"],
        },
        BACK: { target: "naming" },
        GOTO: [
          { guard: "gotoNaming", target: "naming" },
          { guard: "gotoReviewing", target: "reviewing" },
        ],
      },
    },
    reviewing: {
      entry: "trackReviewReached",
      on: {
        SUBMIT: { guard: "canSubmit", target: "submitting" },
        BACK: { target: "editingItems" },
        GOTO: [
          { guard: "gotoNaming", target: "naming" },
          { guard: "gotoEditingItems", target: "editingItems" },
        ],
      },
    },
    submitting: {
      entry: [assign({ error: undefined }), "trackSubmitted"],
      invoke: {
        id: "runMint",
        src: "runMint",
        input: ({ context }) => ({
          name: context.name,
          type: context.type,
          items: context.items,
          mint: context.mint,
        }),
        onDone: {
          target: "done",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "create failed"),
          }),
        },
      },
    },
    done: {
      type: "final",
    },
    error: {
      on: {
        RETRY: { guard: "canSubmit", target: "submitting" },
        GOTO: [
          { guard: "gotoNaming", target: "naming" },
          { guard: "gotoEditingItems", target: "editingItems" },
        ],
      },
    },
  },
});

export type CreateCollectionMachine = typeof createCollectionMachine;

export function resolveCreateSnapshot(args: {
  step: CreateStateId;
  trackCtx: TrackContext;
  feePerItem?: number;
  mint?: MintFn;
  track?: TrackFn;
  seed?: { name?: string; type?: CollectionType; items?: DraftItem[] };
}) {
  const { step, trackCtx, feePerItem, mint, track, seed } = args;
  if (step === "naming" && !seed?.name) return undefined;

  const context: CreateCollectionContext = {
    trackCtx,
    feePerItem: feePerItem ?? DEFAULT_FEE_PER_ITEM,
    mint: mint ?? simulateMint,
    track: track ?? defaultTrack,
    name: seed?.name ?? "",
    type: seed?.type ?? "standard",
    items: seed?.items ?? [],
  };
  return createCollectionMachine.resolveState({ value: step, context });
}
