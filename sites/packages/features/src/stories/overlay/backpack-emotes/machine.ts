import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type SlotBinding = { slot: number; urn: string; name: string };

export type SaveResult = { entityId: string; count: number };

export type SaveFn = (args: {
  loadout: SlotBinding[];
  signal?: AbortSignal;
}) => Promise<SaveResult>;

export type EmotesInput = {
  trackCtx: TrackContext;
  loadout?: SlotBinding[];
  save?: SaveFn;
  track?: TrackFn;
};

export type EmotesContext = {
  trackCtx: TrackContext;
  save: SaveFn;
  track: TrackFn;
  loadout: SlotBinding[];
  activeSlot?: number;
  pendingUrn?: string;
  pendingName?: string;
  result?: SaveResult;
  error?: string;
};

export type EmotesEvent =
  | { type: "OPEN" }
  | { type: "PICK_SLOT"; slot: number }
  | { type: "ASSIGN"; urn: string; name?: string }
  | { type: "CONFIRM" }
  | { type: "REVIEW" }
  | { type: "SAVE" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const EMOTES_EVENTS = {
  started: "cl_emotes_started",
  slotPicked: "cl_emotes_slot_picked",
  browse: "cl_emotes_browse",
  assigned: "cl_emotes_assigned",
  review: "cl_emotes_review",
  saved: "cl_emotes_saved",
  done: "cl_emotes_done",
} as const;

export const STATE_TO_SLUG = {
  opening: "open",
  picking: "pick",
  browsing: "browse",
  assigning: "assign",
  reviewing: "review",
  saving: "save",
  done: "done",
  error: "error",
} as const;

export type EmotesStateId = keyof typeof STATE_TO_SLUG;
export type EmotesStepSlug = (typeof STATE_TO_SLUG)[EmotesStateId];

export const FIRST_STEP_SLUG: EmotesStepSlug = STATE_TO_SLUG.opening;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "opening");

export const SLUG_TO_STATE: Record<EmotesStepSlug, EmotesStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => EmotesStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => EmotesStateId = stepSlugs.toState;

export function resolveStep(
  step: string | null | undefined,
  slot: number | null | undefined,
): EmotesStateId {
  if (step) return slugToState(step);
  if (slot != null) return "picking";
  return "opening";
}

export const simulateSave: SaveFn = async ({ loadout, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return {
    entityId: `bafkrei${loadout.length.toString(16).padStart(4, "0")}simulated`,
    count: loadout.length,
  };
};

export const emotesMachine = setup({
  types: {
    context: {} as EmotesContext,
    events: {} as EmotesEvent,
    input: {} as EmotesInput,
  },
  actors: {
    runSave: fromPromise<SaveResult, { loadout: SlotBinding[]; save: SaveFn }>(
      ({ input, signal }) => input.save({ loadout: input.loadout, signal }),
    ),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(EMOTES_EVENTS.started, {}, context.trackCtx),
    setSlot: assign({
      activeSlot: ({ event }) =>
        event.type === "PICK_SLOT" ? event.slot : undefined,
    }),
    trackSlotPicked: ({ context }) =>
      context.track(
        EMOTES_EVENTS.slotPicked,
        { slot: context.activeSlot },
        context.trackCtx,
      ),
    trackBrowse: ({ context }) =>
      context.track(
        EMOTES_EVENTS.browse,
        { slot: context.activeSlot },
        context.trackCtx,
      ),
    stageEmote: assign({
      pendingUrn: ({ event }) => (event.type === "ASSIGN" ? event.urn : undefined),
      pendingName: ({ event }) =>
        event.type === "ASSIGN" ? (event.name ?? "") : undefined,
    }),
    trackAssigned: ({ context }) =>
      context.track(
        EMOTES_EVENTS.assigned,
        { slot: context.activeSlot, urn: context.pendingUrn },
        context.trackCtx,
      ),
    commitBinding: assign({
      loadout: ({ context }) => {
        const { activeSlot, pendingUrn, pendingName, loadout } = context;
        if (activeSlot == null || !pendingUrn) return loadout;
        const next = loadout.filter((b) => b.slot !== activeSlot);
        next.push({ slot: activeSlot, urn: pendingUrn, name: pendingName ?? "" });
        const rank = (s: number) => (s === 0 ? 10 : s);
        return next.sort((a, b) => rank(a.slot) - rank(b.slot));
      },
      pendingUrn: undefined,
      pendingName: undefined,
      activeSlot: undefined,
    }),
    trackReview: ({ context }) =>
      context.track(
        EMOTES_EVENTS.review,
        { count: context.loadout.length },
        context.trackCtx,
      ),
    trackSaved: ({ context }) => {
      context.track(
        EMOTES_EVENTS.saved,
        { count: context.result?.count ?? context.loadout.length, stub: true },
        context.trackCtx,
      );
      context.track(EMOTES_EVENTS.done, {}, context.trackCtx);
    },
  },
}).createMachine({
  id: "backpackEmotes",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    save: input.save ?? simulateSave,
    track: input.track ?? defaultTrack,
    loadout: input.loadout ?? [],
  }),
  initial: "opening",
  states: {
    opening: {
      on: {
        OPEN: { target: "picking", actions: "trackStarted" },
      },
    },
    picking: {
      on: {
        PICK_SLOT: { target: "browsing", actions: ["setSlot", "trackSlotPicked"] },
        REVIEW: { target: "reviewing" },
      },
    },
    browsing: {
      entry: "trackBrowse",
      on: {
        ASSIGN: { target: "assigning", actions: ["stageEmote", "trackAssigned"] },
        BACK: { target: "picking" },
      },
    },
    assigning: {
      on: {
        CONFIRM: { target: "picking", actions: "commitBinding" },
        BACK: { target: "browsing" },
      },
    },
    reviewing: {
      entry: "trackReview",
      on: {
        SAVE: { target: "saving" },
        BACK: { target: "picking" },
      },
    },
    saving: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSave",
        src: "runSave",
        input: ({ context }) => ({ loadout: context.loadout, save: context.save }),
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

export type EmotesMachine = typeof emotesMachine;

export function resolveEmotesSnapshot(args: {
  step: EmotesStateId;
  trackCtx: TrackContext;
  loadout: SlotBinding[];
  save?: SaveFn;
  track?: TrackFn;
  slot?: number;
  urn?: string;
  name?: string;
}) {
  const { step, trackCtx, loadout, save, track, slot, urn, name } = args;
  if (step === "opening") return undefined;
  const context: EmotesContext = {
    trackCtx,
    save: save ?? simulateSave,
    track: track ?? defaultTrack,
    loadout,
    activeSlot: step === "browsing" || step === "assigning" ? slot : undefined,
    pendingUrn: step === "assigning" ? urn : undefined,
    pendingName: step === "assigning" ? name : undefined,
  };
  return emotesMachine.resolveState({ value: step, context });
}
