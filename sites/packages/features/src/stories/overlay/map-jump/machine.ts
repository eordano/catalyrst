import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import { buildJumpUrl, type MapPin, type PinCategory } from "@data/lib/catalyst/overlay/map-jump";

export type { TrackFn };

export type JumpResult = { jumpUrl: string };

export type JumpFn = (args: {
  pin: MapPin;
  signal?: AbortSignal;
}) => Promise<JumpResult>;

export type MapJumpInput = {
  trackCtx: TrackContext;
  filter?: PinCategory;
  pin?: MapPin | null;
  jump?: JumpFn;
  track?: TrackFn;
};

export type MapJumpContext = {
  trackCtx: TrackContext;
  filter: PinCategory;
  pin?: MapPin | null;
  setHome: boolean;
  jump: JumpFn;
  track: TrackFn;
  result?: JumpResult;
  error?: string;
};

export type MapJumpEvent =
  | { type: "FILTER"; filter: PinCategory }
  | { type: "SELECT_PIN"; pin: MapPin }
  | { type: "CLEAR" }
  | { type: "CONFIRM" }
  | { type: "TOGGLE_HOME" }
  | { type: "BACK" }
  | { type: "JUMP" }
  | { type: "RETRY" };

export const MAP_JUMP_EVENTS = {
  opened: "cl_map_opened",
  filtered: "cl_map_filtered",
  pinSelected: "cl_map_pin_selected",
  confirmReached: "cl_map_confirm_reached",
  jump: "cl_map_jump",
  done: "cl_map_jump_done",
} as const;

export const STATE_TO_SLUG = {
  browsing: "map",
  selected: "select",
  confirming: "confirm",
  jumping: "jump",
  done: "done",
  error: "error",
} as const;

export type MapJumpStateId = keyof typeof STATE_TO_SLUG;
export type MapJumpStepSlug = (typeof STATE_TO_SLUG)[MapJumpStateId];

export const FIRST_STEP_SLUG: MapJumpStepSlug = STATE_TO_SLUG.browsing;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "browsing");

export const SLUG_TO_STATE: Record<MapJumpStepSlug, MapJumpStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => MapJumpStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => MapJumpStateId = stepSlugs.toState;

export const simulateJump: JumpFn = async ({ pin, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { jumpUrl: buildJumpUrl(pin) };
};

export const mapJumpMachine = setup({
  types: {
    context: {} as MapJumpContext,
    events: {} as MapJumpEvent,
    input: {} as MapJumpInput,
  },
  actors: {
    runJump: fromPromise<JumpResult, { pin: MapPin; jump: JumpFn }>(
      ({ input, signal }) => input.jump({ pin: input.pin, signal }),
    ),
  },
  actions: {
    setFilter: assign({
      filter: ({ context, event }) =>
        event.type === "FILTER" ? event.filter : context.filter,
    }),
    trackFiltered: ({ context, event }) => {
      if (event.type !== "FILTER") return;
      context.track(MAP_JUMP_EVENTS.filtered, { filter: event.filter }, context.trackCtx);
    },
    setPin: assign({
      pin: ({ context, event }) =>
        event.type === "SELECT_PIN" ? event.pin : context.pin,
    }),
    trackPinSelected: ({ context, event }) => {
      if (event.type !== "SELECT_PIN") return;
      context.track(
        MAP_JUMP_EVENTS.pinSelected,
        { place_id: event.pin.id, coords: event.pin.coords },
        context.trackCtx,
      );
    },
    toggleHome: assign({ setHome: ({ context }) => !context.setHome }),
    trackConfirmReached: ({ context }) =>
      context.track(
        MAP_JUMP_EVENTS.confirmReached,
        { coords: context.pin?.coords, set_home: context.setHome },
        context.trackCtx,
      ),
    trackJump: ({ context }) =>
      context.track(
        MAP_JUMP_EVENTS.jump,
        {
          place_id: context.pin?.id,
          coords: context.pin?.coords,
          jump_url: context.result?.jumpUrl,
          set_home: context.setHome,
          simulated: true,
        },
        context.trackCtx,
      ),
    trackDone: ({ context }) =>
      context.track(
        MAP_JUMP_EVENTS.done,
        { place_id: context.pin?.id, coords: context.pin?.coords },
        context.trackCtx,
      ),
  },
  guards: {
    hasPin: ({ context }) => Boolean(context.pin),
  },
}).createMachine({
  id: "mapJump",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    filter: input.filter ?? "all",
    pin: input.pin ?? null,
    setHome: false,
    jump: input.jump ?? simulateJump,
    track: input.track ?? defaultTrack,
  }),
  initial: "browsing",
  states: {
    browsing: {
      on: {
        FILTER: { actions: ["setFilter", "trackFiltered"] },
        SELECT_PIN: { target: "selected", actions: ["setPin", "trackPinSelected"] },
      },
    },
    selected: {
      on: {
        FILTER: { actions: ["setFilter", "trackFiltered"] },
        SELECT_PIN: { actions: ["setPin", "trackPinSelected"] },
        TOGGLE_HOME: { actions: "toggleHome" },
        CLEAR: { target: "browsing", actions: assign({ pin: () => null }) },
        CONFIRM: { target: "confirming", guard: "hasPin" },
      },
    },
    confirming: {
      entry: "trackConfirmReached",
      on: {
        TOGGLE_HOME: { actions: "toggleHome" },
        BACK: { target: "selected" },
        JUMP: { target: "jumping" },
      },
    },
    jumping: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runJump",
        src: "runJump",
        input: ({ context }) => ({
          pin: context.pin as MapPin,
          jump: context.jump,
        }),
        onDone: {
          target: "done",
          actions: [assign({ result: ({ event }) => event.output }), "trackJump"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "teleport failed"),
          }),
        },
      },
    },
    done: {
      entry: "trackDone",
      type: "final",
    },
    error: {
      on: {
        RETRY: { target: "jumping" },
      },
    },
  },
});

export type MapJumpMachine = typeof mapJumpMachine;

export function resolveMapJumpSnapshot(args: {
  step: MapJumpStateId;
  trackCtx: TrackContext;
  filter?: PinCategory;
  pin?: MapPin | null;
  jump?: JumpFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, filter = "all", pin = null, jump, track } = args;
  if (step === "browsing") return undefined;
  const context: MapJumpContext = {
    trackCtx,
    filter,
    pin,
    setHome: false,
    jump: jump ?? simulateJump,
    track: track ?? defaultTrack,
  };
  return mapJumpMachine.resolveState({ value: step, context });
}
