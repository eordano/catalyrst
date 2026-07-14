import { delay, makeStepSlugs } from "@core/lib/stories/index";
import { toErrorMessage } from "@core/lib/errors";
import { assign, fromPromise, setup } from "xstate";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type FulfillResult = {
  checkoutId: number;
  status: string;
  phase: "done" | "failed" | "pending";
};

export type FulfillFn = (args: {
  idempotencyKey: string;
  signal?: AbortSignal;
}) => Promise<FulfillResult>;

export type CheckoutInput = {
  totalCredits: string;
  idempotencyKey: string;
  trackCtx: TrackContext;
  run?: FulfillFn;
  track?: TrackFn;
};

export type CheckoutContext = {
  totalCredits: string;
  idempotencyKey: string;
  trackCtx: TrackContext;
  run: FulfillFn;
  track: TrackFn;
  result?: FulfillResult;
  error?: string;
};

export type CheckoutEvent =
  | { type: "CONFIRM" }
  | { type: "RETRY" };

export const CHECKOUT_EVENTS = {
  started: "mk_checkout_started",
  confirmReached: "mk_checkout_confirm_reached",
  succeeded: "mk_checkout_succeeded",
  failed: "mk_checkout_failed",
  processing: "mk_checkout_processing",
} as const;

export const STATE_TO_SLUG = {
  review: "review",
  fulfilling: "fulfilling",
  done: "done",
  processing: "processing",
  failed: "failed",
} as const;

export type CheckoutStateId = keyof typeof STATE_TO_SLUG;
export type CheckoutStepSlug = (typeof STATE_TO_SLUG)[CheckoutStateId];

export const FIRST_STEP_SLUG: CheckoutStepSlug = STATE_TO_SLUG.review;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "review");

export const SLUG_TO_STATE: Record<CheckoutStepSlug, CheckoutStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => CheckoutStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => CheckoutStateId = stepSlugs.toState;

export const simulateFulfill: FulfillFn = async ({ signal }) => {
  await delay(300, signal);
  return { checkoutId: 0, status: "fulfilled", phase: "done" };
};

export const checkoutMachine = setup({
  types: {
    context: {} as CheckoutContext,
    events: {} as CheckoutEvent,
    input: {} as CheckoutInput,
  },
  actors: {
    runFulfill: fromPromise<FulfillResult, { run: FulfillFn; idempotencyKey: string }>(
      ({ input, signal }) =>
        input.run({ idempotencyKey: input.idempotencyKey, signal }),
    ),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(
        CHECKOUT_EVENTS.started,
        { total_credits: context.totalCredits },
        context.trackCtx,
      ),
    trackConfirmReached: ({ context }) =>
      context.track(
        CHECKOUT_EVENTS.confirmReached,
        { total_credits: context.totalCredits },
        context.trackCtx,
      ),
    trackSucceeded: ({ context }) =>
      context.track(
        CHECKOUT_EVENTS.succeeded,
        {
          total_credits: context.totalCredits,
          checkout_id: context.result?.checkoutId,
          status: context.result?.status,
        },
        context.trackCtx,
      ),
    trackFailed: ({ context }) =>
      context.track(
        CHECKOUT_EVENTS.failed,
        {
          total_credits: context.totalCredits,
          status: context.result?.status,
          error: context.error,
        },
        context.trackCtx,
      ),
    trackProcessing: ({ context }) =>
      context.track(
        CHECKOUT_EVENTS.processing,
        {
          total_credits: context.totalCredits,
          checkout_id: context.result?.checkoutId,
          status: context.result?.status,
        },
        context.trackCtx,
      ),
    setError: assign({
      error: ({ event }) => {
        const e = event as { error?: unknown };
        return toErrorMessage(e.error, "checkout failed");
      },
    }),
    clearError: assign({ error: undefined }),
  },
  guards: {
    isDone: ({ event }) =>
      (event as { output?: FulfillResult }).output?.phase === "done",
    isPending: ({ event }) =>
      (event as { output?: FulfillResult }).output?.phase === "pending",
  },
}).createMachine({
  id: "checkoutWizard",
  context: ({ input }) => ({
    totalCredits: input.totalCredits,
    idempotencyKey: input.idempotencyKey,
    trackCtx: input.trackCtx,
    run: input.run ?? simulateFulfill,
    track: input.track ?? defaultTrack,
  }),
  initial: "review",
  states: {
    review: {
      on: {
        CONFIRM: { target: "fulfilling", actions: "trackStarted" },
      },
    },
    fulfilling: {
      entry: ["clearError", "trackConfirmReached"],
      invoke: {
        id: "runFulfill",
        src: "runFulfill",
        input: ({ context }) => ({
          run: context.run,
          idempotencyKey: context.idempotencyKey,
        }),
        onDone: [
          {
            target: "done",
            guard: "isDone",
            actions: [
              assign({ result: ({ event }) => event.output }),
              "trackSucceeded",
            ],
          },
          {
            target: "processing",
            guard: "isPending",
            actions: [
              assign({ result: ({ event }) => event.output }),
              "trackProcessing",
            ],
          },
          {
            target: "failed",
            actions: [
              assign({ result: ({ event }) => event.output }),
              "trackFailed",
            ],
          },
        ],
        onError: { target: "failed", actions: ["setError", "trackFailed"] },
      },
    },
    done: {
      type: "final",
    },
    processing: {},
    failed: {
      on: {
        RETRY: { target: "fulfilling" },
      },
    },
  },
});

export type CheckoutMachine = typeof checkoutMachine;

export function resolveCheckoutSnapshot(args: {
  step: CheckoutStateId;
  totalCredits: string;
  idempotencyKey: string;
  trackCtx: TrackContext;
  run?: FulfillFn;
  track?: TrackFn;
}) {
  const { step, totalCredits, idempotencyKey, trackCtx, run, track } = args;
  if (step === "review") return undefined;
  const context: CheckoutContext = {
    totalCredits,
    idempotencyKey,
    trackCtx,
    run: run ?? simulateFulfill,
    track: track ?? defaultTrack,
  };
  return checkoutMachine.resolveState({ value: step, context });
}
