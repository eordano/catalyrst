import { assign, fromPromise, setup } from "xstate";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  canSaveToSlot,
  type EquippedSet,
} from "@data/lib/catalyst/overlay/outfit-save";

export type { TrackFn };

export type SaveResult = { slot: number; name: string; simulated: true };

export type SaveFn = (args: {
  slot: number;
  name: string;
  outfit: EquippedSet;
  signal?: AbortSignal;
}) => Promise<SaveResult>;

export type OutfitSaveSeed = {
  equipped: EquippedSet;
  freeSlots: number;
  totalSlots: number;
  namesForExtraSlots: string[];
};

export type OutfitSaveInput = {
  trackCtx: TrackContext;
  seed: OutfitSaveSeed;
  save?: SaveFn;
  track?: TrackFn;
};

export type OutfitSaveContext = {
  trackCtx: TrackContext;
  seed: OutfitSaveSeed;
  save: SaveFn;
  track: TrackFn;
  slot: number;
  name: string;
  captured?: EquippedSet;
  gateReason?: string;
  result?: SaveResult;
  error?: string;
};

export type OutfitSaveEvent =
  | { type: "OPEN_SLOT"; slot: number }
  | { type: "SET_NAME"; name: string }
  | { type: "NEXT" }
  | { type: "CAPTURE" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const OUTFIT_EVENTS = {
  started: "cl_outfit_save_started",
  named: "cl_outfit_named",
  captured: "cl_outfit_captured",
  gated: "cl_outfit_slot_gated",
  saved: "cl_outfit_saved",
  completed: "cl_outfit_save_completed",
} as const;

export const STATE_TO_SLUG = {
  browsing: "browse",
  naming: "name",
  capturing: "capture",
  saving: "save",
  gated: "gated",
  done: "done",
} as const;

export type OutfitStateId = keyof typeof STATE_TO_SLUG;
export type OutfitStepSlug = (typeof STATE_TO_SLUG)[OutfitStateId];

export const FIRST_STEP_SLUG: OutfitStepSlug = STATE_TO_SLUG.browsing;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "browsing");

export const SLUG_TO_STATE: Record<OutfitStepSlug, OutfitStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => OutfitStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => OutfitStateId = stepSlugs.toState;

export const simulateSave: SaveFn = async ({ slot, name, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 350);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { slot, name, simulated: true };
};

export const outfitSaveMachine = setup({
  types: {
    context: {} as OutfitSaveContext,
    events: {} as OutfitSaveEvent,
    input: {} as OutfitSaveInput,
  },
  actors: {
    runSave: fromPromise<
      SaveResult,
      { slot: number; name: string; outfit: EquippedSet; save: SaveFn }
    >(({ input, signal }) =>
      input.save({ slot: input.slot, name: input.name, outfit: input.outfit, signal }),
    ),
  },
  guards: {
    canSave: ({ context }) =>
      canSaveToSlot({
        slot: context.slot,
        name: context.name,
        freeSlots: context.seed.freeSlots,
        totalSlots: context.seed.totalSlots,
        namesForExtraSlots: context.seed.namesForExtraSlots,
      }).ok,
  },
  actions: {
    setSlot: assign({
      slot: ({ event }) => (event.type === "OPEN_SLOT" ? event.slot : 0),
    }),
    setName: assign({
      name: ({ context, event }) =>
        event.type === "SET_NAME" ? event.name : context.name,
    }),
    capture: assign({
      captured: ({ context }) => ({ ...context.seed.equipped }),
    }),
    trackStarted: ({ context, event }) => {
      if (event.type !== "OPEN_SLOT") return;
      context.track(OUTFIT_EVENTS.started, { slot: event.slot }, context.trackCtx);
    },
    trackNamed: ({ context }) =>
      context.track(
        OUTFIT_EVENTS.named,
        { slot: context.slot, name: context.name },
        context.trackCtx,
      ),
    trackCaptured: ({ context }) =>
      context.track(
        OUTFIT_EVENTS.captured,
        { slot: context.slot, wearables: context.captured?.wearables.length ?? 0 },
        context.trackCtx,
      ),
    trackGated: assign({
      gateReason: ({ context }) => {
        const res = canSaveToSlot({
          slot: context.slot,
          name: context.name,
          freeSlots: context.seed.freeSlots,
          totalSlots: context.seed.totalSlots,
          namesForExtraSlots: context.seed.namesForExtraSlots,
        });
        context.track(
          OUTFIT_EVENTS.gated,
          { slot: context.slot, reason: res.reason ?? "blocked" },
          context.trackCtx,
        );
        return res.reason ?? "blocked";
      },
    }),
    trackSaved: ({ context }) => {
      context.track(
        OUTFIT_EVENTS.saved,
        {
          slot: context.slot,
          name: context.name,
          wearables: context.captured?.wearables.length ?? 0,
          simulated: true,
        },
        context.trackCtx,
      );
      context.track(
        OUTFIT_EVENTS.completed,
        { slot: context.slot },
        context.trackCtx,
      );
    },
  },
}).createMachine({
  id: "outfitSave",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    seed: input.seed,
    save: input.save ?? simulateSave,
    track: input.track ?? defaultTrack,
    slot: 0,
    name: "",
  }),
  initial: "browsing",
  states: {
    browsing: {
      on: {
        OPEN_SLOT: {
          target: "naming",
          actions: ["setSlot", "trackStarted"],
        },
      },
    },
    naming: {
      on: {
        SET_NAME: { actions: "setName" },
        NEXT: { target: "capturing", actions: "trackNamed" },
        BACK: { target: "browsing" },
      },
    },
    capturing: {
      on: {
        CAPTURE: { target: "saving", actions: ["capture", "trackCaptured"] },
        BACK: { target: "naming" },
      },
    },
    saving: {
      always: [{ guard: "canSave", target: "persisting" }, { target: "gated" }],
    },
    persisting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSave",
        src: "runSave",
        input: ({ context }) => ({
          slot: context.slot,
          name: context.name,
          outfit: context.captured ?? context.seed.equipped,
          save: context.save,
        }),
        onDone: {
          target: "done",
          actions: [assign({ result: ({ event }) => event.output }), "trackSaved"],
        },
        onError: {
          target: "gated",
          actions: assign({
            gateReason: "save-failed",
            error: ({ event }) =>
              event.error instanceof Error ? event.error.message : "save failed",
          }),
        },
      },
    },
    gated: {
      entry: "trackGated",
      on: {
        BACK: { target: "naming" },
        RETRY: { target: "saving" },
      },
    },
    done: {
      type: "final",
    },
  },
});

export type OutfitSaveMachine = typeof outfitSaveMachine;

export function valueToSlug(value: string): OutfitStepSlug {
  if (value === "persisting") return STATE_TO_SLUG.saving;
  return stateToSlug(value);
}

export function resolveOutfitSnapshot(args: {
  step: OutfitStateId;
  trackCtx: TrackContext;
  seed: OutfitSaveSeed;
  save?: SaveFn;
  track?: TrackFn;
  slot?: number;
  name?: string;
}) {
  const { step, trackCtx, seed, save, track, slot = 0, name = "" } = args;
  if (step === "browsing") return undefined;
  const captured = step === "capturing" ? undefined : { ...seed.equipped };
  const context: OutfitSaveContext = {
    trackCtx,
    seed,
    save: save ?? simulateSave,
    track: track ?? defaultTrack,
    slot,
    name,
    captured,
  };
  return outfitSaveMachine.resolveState({ value: step, context });
}
