import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type ColorKind = "skin" | "hair" | "eye";

export type { TrackFn };

export type SaveResult = { entityId: string; deployed?: boolean };

export type SaveFn = (args: {
  wearables: string[];
  colors: Record<ColorKind, string>;
  signal?: AbortSignal;
}) => Promise<SaveResult>;

export type BackpackInput = {
  trackCtx: TrackContext;
  baseWearables?: string[];
  baseColors?: Partial<Record<ColorKind, string>>;
  ownedEmpty?: boolean;
  save?: SaveFn;
  track?: TrackFn;
};

export type BackpackContext = {
  trackCtx: TrackContext;
  save: SaveFn;
  track: TrackFn;
  ownedEmpty: boolean;
  baseWearables: string[];
  wearables: string[];
  colors: Record<ColorKind, string>;
  selectedUrn?: string;
  selectedCategory?: string;
  selectedRarity?: string | null;
  result?: SaveResult;
  error?: string;
};

export type BackpackEvent =
  | { type: "OPEN" }
  | { type: "SELECT"; urn: string; category: string; rarity: string | null }
  | { type: "INVENTORY_EMPTY" }
  | { type: "EQUIP"; urn: string; slot: string; wearables: string[] }
  | { type: "PICK_COLOR"; kind: ColorKind; color: string }
  | { type: "REVIEW" }
  | { type: "SAVE" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const BACKPACK_EVENTS = {
  opened: "cl_backpack_opened",
  browsed: "cl_backpack_browsed",
  inventoryEmpty: "cl_backpack_inventory_empty",
  selected: "cl_backpack_selected",
  equipped: "cl_backpack_equipped",
  colorChanged: "cl_backpack_color_changed",
  reviewReached: "cl_backpack_review_reached",
  saved: "cl_backpack_saved",
  done: "cl_backpack_done",
} as const;

const DEFAULT_COLORS: Record<ColorKind, string> = {
  skin: "#c98c63",
  hair: "#5c3824",
  eye: "#3a6ea5",
};

export const STATE_TO_SLUG = {
  opening: "open",
  browsing: "browse",
  selecting: "select",
  equipping: "equip",
  coloring: "color",
  reviewing: "review",
  saving: "save",
  done: "done",
  error: "error",
} as const;

export type BackpackStateId = keyof typeof STATE_TO_SLUG;
export type BackpackStepSlug = (typeof STATE_TO_SLUG)[BackpackStateId];

export const FIRST_STEP_SLUG: BackpackStepSlug = STATE_TO_SLUG.opening;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "opening");

export const SLUG_TO_STATE: Record<BackpackStepSlug, BackpackStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => BackpackStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => BackpackStateId = stepSlugs.toState;

export const simulateSave: SaveFn = async ({ wearables, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const seed = wearables.join("|");
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return { entityId: `bafkrei-sim-${h.toString(16).padStart(8, "0")}` };
};

export const backpackMachine = setup({
  types: {
    context: {} as BackpackContext,
    events: {} as BackpackEvent,
    input: {} as BackpackInput,
  },
  actors: {
    runSave: fromPromise<
      SaveResult,
      { wearables: string[]; colors: Record<ColorKind, string>; save: SaveFn }
    >(({ input, signal }) =>
      input.save({ wearables: input.wearables, colors: input.colors, signal }),
    ),
  },
  actions: {
    trackOpened: ({ context }) => {
      context.track(BACKPACK_EVENTS.opened, {}, context.trackCtx);
      context.track(
        BACKPACK_EVENTS.browsed,
        { empty: context.ownedEmpty },
        context.trackCtx,
      );
    },
    trackInventoryEmpty: ({ context }) =>
      context.track(BACKPACK_EVENTS.inventoryEmpty, {}, context.trackCtx),
    setSelected: assign(({ event }) =>
      event.type === "SELECT"
        ? {
            selectedUrn: event.urn,
            selectedCategory: event.category,
            selectedRarity: event.rarity,
          }
        : {},
    ),
    trackSelected: ({ context, event }) => {
      if (event.type !== "SELECT") return;
      context.track(
        BACKPACK_EVENTS.selected,
        { urn: event.urn, category: event.category, rarity: event.rarity },
        context.trackCtx,
      );
    },
    setEquipped: assign(({ event }) =>
      event.type === "EQUIP" ? { wearables: event.wearables } : {},
    ),
    trackEquipped: ({ context, event }) => {
      if (event.type !== "EQUIP") return;
      context.track(
        BACKPACK_EVENTS.equipped,
        { urn: event.urn, slot: event.slot },
        context.trackCtx,
      );
    },
    setColor: assign(({ context, event }) => {
      if (event.type !== "PICK_COLOR") return {};
      return { colors: { ...context.colors, [event.kind]: event.color } };
    }),
    trackColor: ({ context, event }) => {
      if (event.type !== "PICK_COLOR") return;
      context.track(
        BACKPACK_EVENTS.colorChanged,
        { kind: event.kind, color: event.color },
        context.trackCtx,
      );
    },
    trackReviewReached: ({ context }) =>
      context.track(BACKPACK_EVENTS.reviewReached, {}, context.trackCtx),
    trackSaved: ({ context }) => {
      context.track(
        BACKPACK_EVENTS.saved,
        {
          entity_id: context.result?.entityId,
          count: context.wearables.length,
          deployed: context.result?.deployed ?? false,
        },
        context.trackCtx,
      );
      context.track(BACKPACK_EVENTS.done, {}, context.trackCtx);
    },
  },
}).createMachine({
  id: "backpackEquip",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    save: input.save ?? simulateSave,
    track: input.track ?? defaultTrack,
    ownedEmpty: input.ownedEmpty ?? true,
    baseWearables: input.baseWearables ?? [],
    wearables: input.baseWearables ?? [],
    colors: { ...DEFAULT_COLORS, ...(input.baseColors ?? {}) },
  }),
  initial: "opening",
  states: {
    opening: {
      on: {
        OPEN: { target: "browsing" },
      },
    },
    browsing: {
      entry: "trackOpened",
      on: {
        SELECT: { target: "selecting", actions: ["setSelected", "trackSelected"] },
        INVENTORY_EMPTY: { actions: "trackInventoryEmpty" },
      },
    },
    selecting: {
      on: {
        EQUIP: { target: "equipping", actions: ["setEquipped", "trackEquipped"] },
        SELECT: { actions: ["setSelected", "trackSelected"] },
        BACK: { target: "browsing" },
      },
    },
    equipping: {
      on: {
        PICK_COLOR: { target: "coloring", actions: ["setColor", "trackColor"] },
        SELECT: { target: "selecting", actions: ["setSelected", "trackSelected"] },
        REVIEW: { target: "reviewing" },
        BACK: { target: "browsing" },
      },
    },
    coloring: {
      on: {
        PICK_COLOR: { actions: ["setColor", "trackColor"] },
        REVIEW: { target: "reviewing" },
        BACK: { target: "equipping" },
      },
    },
    reviewing: {
      entry: "trackReviewReached",
      on: {
        SAVE: { target: "saving" },
        BACK: { target: "browsing" },
      },
    },
    saving: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSave",
        src: "runSave",
        input: ({ context }) => ({
          wearables: context.wearables,
          colors: context.colors,
          save: context.save,
        }),
        onDone: {
          target: "done",
          actions: [assign({ result: ({ event }) => event.output }), "trackSaved"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "save failed"),
          }),
        },
      },
    },
    done: {
      type: "final",
    },
    error: {
      on: {
        RETRY: { target: "saving" },
      },
    },
  },
});

export type BackpackMachine = typeof backpackMachine;

export function resolveBackpackSnapshot(args: {
  step: BackpackStateId;
  trackCtx: TrackContext;
  baseWearables?: string[];
  baseColors?: Partial<Record<ColorKind, string>>;
  ownedEmpty?: boolean;
  save?: SaveFn;
  track?: TrackFn;
  selected?: { urn: string; category: string; rarity: string | null };
}) {
  const {
    step,
    trackCtx,
    baseWearables = [],
    baseColors,
    ownedEmpty = true,
    save,
    track,
    selected,
  } = args;
  if (step === "opening") return undefined;
  const context: BackpackContext = {
    trackCtx,
    save: save ?? simulateSave,
    track: track ?? defaultTrack,
    ownedEmpty,
    baseWearables,
    wearables: baseWearables,
    colors: { ...DEFAULT_COLORS, ...(baseColors ?? {}) },
    selectedUrn: selected?.urn,
    selectedCategory: selected?.category,
    selectedRarity: selected?.rarity,
  };
  return backpackMachine.resolveState({ value: step, context });
}
