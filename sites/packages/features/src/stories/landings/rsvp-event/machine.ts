import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type RsvpDirection = "going" | "cancel";

export type { TrackFn };

export type RsvpResult = { count: number };

export type CommitFn = (args: {
  eventId: string;
  direction: RsvpDirection;
  count: number;
  signal?: AbortSignal;
}) => Promise<RsvpResult>;

export type RsvpInput = {
  trackCtx: TrackContext;
  eventId: string;
  count?: number;
  initiallyGoing?: boolean;
  commit?: CommitFn;
  track?: TrackFn;
};

export type RsvpContext = {
  trackCtx: TrackContext;
  eventId: string;
  count: number;
  commit: CommitFn;
  track: TrackFn;
  error?: string;
};

export type RsvpEvent =
  | { type: "TAP_GOING" }
  | { type: "SIGN_IN" }
  | { type: "CONFIRM" }
  | { type: "BACK" }
  | { type: "CANCEL" }
  | { type: "CANCEL_RSVP" }
  | { type: "RETRY" }
  | { type: "DISMISS" };

export const RSVP_EVENTS = {
  started: "lp_rsvp_started",
  signin: "lp_rsvp_signin",
  confirmed: "lp_rsvp_confirmed",
  submitting: "lp_rsvp_submitting",
  going: "lp_rsvp_going",
  cancelling: "lp_rsvp_cancelling",
  cancelled: "lp_rsvp_cancelled",
  error: "lp_rsvp_error",
} as const;

export const STATE_TO_SLUG = {
  idle: "idle",
  signinGate: "signin-gate",
  confirming: "confirming",
  submitting: "submitting",
  going: "going",
  cancelling: "cancelling",
  notGoing: "not-going",
  error: "error",
} as const;

export type RsvpStateId = keyof typeof STATE_TO_SLUG;
export type RsvpStepSlug = (typeof STATE_TO_SLUG)[RsvpStateId];

export const FIRST_STEP_SLUG: RsvpStepSlug = STATE_TO_SLUG.idle;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "idle");

export const SLUG_TO_STATE: Record<RsvpStepSlug, RsvpStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => RsvpStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => RsvpStateId = stepSlugs.toState;

export const simulateCommit: CommitFn = async ({ direction, count, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 350);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const next = direction === "going" ? count + 1 : Math.max(0, count - 1);
  return { count: next };
};

export const rsvpMachine = setup({
  types: {
    context: {} as RsvpContext,
    events: {} as RsvpEvent,
    input: {} as RsvpInput,
  },
  actors: {
    runCommit: fromPromise<
      RsvpResult,
      { eventId: string; direction: RsvpDirection; count: number; commit: CommitFn }
    >(({ input, signal }) =>
      input.commit({
        eventId: input.eventId,
        direction: input.direction,
        count: input.count,
        signal,
      }),
    ),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(RSVP_EVENTS.started, { event_id: context.eventId }, context.trackCtx),
    trackSignin: ({ context }) =>
      context.track(
        RSVP_EVENTS.signin,
        { event_id: context.eventId, simulated: true },
        context.trackCtx,
      ),
    trackConfirmed: ({ context }) =>
      context.track(RSVP_EVENTS.confirmed, { event_id: context.eventId }, context.trackCtx),
    trackSubmitting: ({ context }) =>
      context.track(RSVP_EVENTS.submitting, { event_id: context.eventId }, context.trackCtx),
    trackGoing: ({ context }) =>
      context.track(
        RSVP_EVENTS.going,
        { event_id: context.eventId, count: context.count, stub: true },
        context.trackCtx,
      ),
    trackCancelling: ({ context }) =>
      context.track(RSVP_EVENTS.cancelling, { event_id: context.eventId }, context.trackCtx),
    trackCancelled: ({ context }) =>
      context.track(
        RSVP_EVENTS.cancelled,
        { event_id: context.eventId, count: context.count, stub: true },
        context.trackCtx,
      ),
    trackError: ({ context }) =>
      context.track(
        RSVP_EVENTS.error,
        { event_id: context.eventId, reason: context.error ?? "unknown" },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "rsvpWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    eventId: input.eventId,
    count: input.count ?? 0,
    commit: input.commit ?? simulateCommit,
    track: input.track ?? defaultTrack,
  }),
  initial: "idle",
  states: {
    idle: {
      on: {
        TAP_GOING: { target: "signinGate", actions: "trackStarted" },
      },
    },
    signinGate: {
      on: {
        SIGN_IN: { target: "confirming", actions: "trackSignin" },
        CANCEL: { target: "idle" },
      },
    },
    confirming: {
      entry: "trackConfirmed",
      on: {
        CONFIRM: { target: "submitting" },
        BACK: { target: "idle" },
      },
    },
    submitting: {
      entry: [assign({ error: undefined }), "trackSubmitting"],
      invoke: {
        id: "runCommitGoing",
        src: "runCommit",
        input: ({ context }) => ({
          eventId: context.eventId,
          direction: "going" as const,
          count: context.count,
          commit: context.commit,
        }),
        onDone: {
          target: "going",
          actions: [assign({ count: ({ event }) => event.output.count }), "trackGoing"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "rsvp failed"),
          }),
        },
      },
    },
    going: {
      on: {
        CANCEL_RSVP: { target: "cancelling" },
      },
    },
    cancelling: {
      entry: [assign({ error: undefined }), "trackCancelling"],
      invoke: {
        id: "runCommitCancel",
        src: "runCommit",
        input: ({ context }) => ({
          eventId: context.eventId,
          direction: "cancel" as const,
          count: context.count,
          commit: context.commit,
        }),
        onDone: {
          target: "notGoing",
          actions: [assign({ count: ({ event }) => event.output.count }), "trackCancelled"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "cancel failed"),
          }),
        },
      },
    },
    notGoing: {
      on: {
        TAP_GOING: { target: "signinGate", actions: "trackStarted" },
      },
    },
    error: {
      entry: "trackError",
      on: {
        RETRY: { target: "submitting" },
        DISMISS: { target: "idle" },
      },
    },
  },
});

export type RsvpMachine = typeof rsvpMachine;

export function resolveRsvpSnapshot(args: {
  step: RsvpStateId;
  trackCtx: TrackContext;
  eventId: string;
  count?: number;
  commit?: CommitFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, eventId, count = 0, commit, track } = args;
  if (step === "idle") return undefined;
  const context: RsvpContext = {
    trackCtx,
    eventId,
    count,
    commit: commit ?? simulateCommit,
    track: track ?? defaultTrack,
  };
  return rsvpMachine.resolveState({ value: step, context });
}
