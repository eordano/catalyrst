import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type FriendAction = "request" | "accept" | "cancel" | "reject" | "block";

export type { TrackFn };

export type UpsertResult = { action: FriendAction; address: string };

export type UpsertFn = (args: {
  action: FriendAction;
  address: string;
  signal?: AbortSignal;
}) => Promise<UpsertResult>;

export type FriendInput = {
  trackCtx: TrackContext;
  upsert?: UpsertFn;
  track?: TrackFn;
  action?: FriendAction;
  address?: string;
};

export type FriendContext = {
  trackCtx: TrackContext;
  upsert: UpsertFn;
  track: TrackFn;
  action?: FriendAction;
  address?: string;
  result?: UpsertResult;
  error?: string;
};

export type FriendEvent =
  | { type: "START"; action: FriendAction; address: string }
  | { type: "CONFIRM" }
  | { type: "CANCEL" }
  | { type: "RETRY" };

export const FRIEND_EVENTS = {
  panelOpened: "ov_friend_panel_opened",
  actionStarted: "ov_friend_action_started",
  blockPrompt: "ov_friend_block_prompt",
  blockConfirmed: "ov_friend_block_confirmed",
  actionCompleted: "ov_friend_action_completed",
  actionFailed: "ov_friend_action_failed",
} as const;

export function transitionValid(
  last: FriendAction | undefined,
  to: FriendAction,
): boolean {
  switch (to) {
    case "request":
      return last === undefined || last === "cancel" || last === "reject";
    case "accept":
    case "cancel":
    case "reject":
      return last === "request";
    case "block":
      return (
        last === undefined ||
        last === "request" ||
        last === "cancel" ||
        last === "reject" ||
        last === "accept"
      );
    default:
      return false;
  }
}

export const STATE_TO_SLUG = {
  panel: "panel",
  confirming: "confirm",
  blockPrompt: "block",
  submitting: "submitting",
  done: "done",
  failed: "failed",
} as const;

export type FriendStateId = keyof typeof STATE_TO_SLUG;
export type FriendStepSlug = (typeof STATE_TO_SLUG)[FriendStateId];

export const FIRST_STEP_SLUG: FriendStepSlug = STATE_TO_SLUG.panel;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "panel");

export const SLUG_TO_STATE: Record<FriendStepSlug, FriendStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => FriendStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => FriendStateId = stepSlugs.toState;

export function parseAction(
  raw: string | null | undefined,
): { step: FriendStateId; action?: FriendAction } {
  switch (raw) {
    case "add":
      return { step: "confirming", action: "request" };
    case "accept":
      return { step: "confirming", action: "accept" };
    case "cancel":
      return { step: "confirming", action: "cancel" };
    case "reject":
      return { step: "confirming", action: "reject" };
    case "block":
      return { step: "blockPrompt", action: "block" };
    case "done":
      return { step: "done" };
    default:
      return { step: "panel" };
  }
}

export const simulateUpsert: UpsertFn = async ({ action, address, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 350);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { action, address };
};

export const friendMachine = setup({
  types: {
    context: {} as FriendContext,
    events: {} as FriendEvent,
    input: {} as FriendInput,
  },
  actors: {
    runUpsert: fromPromise<
      UpsertResult,
      { action: FriendAction; address: string; upsert: UpsertFn }
    >(({ input, signal }) =>
      input.upsert({ action: input.action, address: input.address, signal }),
    ),
  },
  actions: {
    setActionTarget: assign({
      action: ({ event }) => (event.type === "START" ? event.action : undefined),
      address: ({ event }) => (event.type === "START" ? event.address : undefined),
    }),
    trackActionStarted: ({ context }) =>
      context.track(
        FRIEND_EVENTS.actionStarted,
        { action: context.action, address: context.address },
        context.trackCtx,
      ),
    trackBlockPrompt: ({ context }) =>
      context.track(
        FRIEND_EVENTS.blockPrompt,
        { address: context.address },
        context.trackCtx,
      ),
    trackBlockConfirmed: ({ context }) =>
      context.track(
        FRIEND_EVENTS.blockConfirmed,
        { address: context.address },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        FRIEND_EVENTS.actionCompleted,
        { action: context.action, address: context.address, stub: true },
        context.trackCtx,
      ),
    trackFailed: ({ context }) =>
      context.track(
        FRIEND_EVENTS.actionFailed,
        { action: context.action, address: context.address, error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "friendRequest",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    upsert: input.upsert ?? simulateUpsert,
    track: input.track ?? defaultTrack,
    action: input.action,
    address: input.address,
  }),
  initial: "panel",
  states: {
    panel: {
      on: {
        START: [
          {
            guard: ({ event }) => event.action === "block",
            target: "blockPrompt",
            actions: "setActionTarget",
          },
          {
            target: "confirming",
            actions: "setActionTarget",
          },
        ],
      },
    },

    confirming: {
      entry: "trackActionStarted",
      on: {
        CONFIRM: { target: "submitting" },
        CANCEL: { target: "panel", actions: assign({ action: undefined, error: undefined }) },
      },
    },

    blockPrompt: {
      entry: "trackBlockPrompt",
      on: {
        CONFIRM: { target: "submitting", actions: "trackBlockConfirmed" },
        CANCEL: { target: "panel", actions: assign({ action: undefined, error: undefined }) },
      },
    },

    submitting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runUpsert",
        src: "runUpsert",
        input: ({ context }) => ({
          action: context.action ?? "request",
          address: context.address ?? "",
          upsert: context.upsert,
        }),
        onDone: {
          target: "done",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: {
          target: "failed",
          actions: [
            assign({
              error: ({ event }) =>
                toErrorMessage(event.error, "upsert failed"),
            }),
            "trackFailed",
          ],
        },
      },
    },

    done: {
      type: "final",
    },

    failed: {
      on: {
        RETRY: { target: "submitting" },
        CANCEL: { target: "panel", actions: assign({ action: undefined, error: undefined }) },
      },
    },
  },
});

export type FriendMachine = typeof friendMachine;

export function resolveFriendSnapshot(args: {
  step: FriendStateId;
  trackCtx: TrackContext;
  upsert?: UpsertFn;
  track?: TrackFn;
  action?: FriendAction;
  address?: string;
}) {
  const { step, trackCtx, upsert, track, action, address } = args;
  if (step === "panel") return undefined;
  const context: FriendContext = {
    trackCtx,
    upsert: upsert ?? simulateUpsert,
    track: track ?? defaultTrack,
    action: step === "blockPrompt" ? "block" : action,
    address,
  };
  return friendMachine.resolveState({ value: step, context });
}
