import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type CancelOrder = {
  orderId: string;
  owner: string;
  /** Null when the listing's wei price could not be read. */
  price: string | null;
  name: string;
  network: "ethereum" | "polygon";
};

export type { TrackFn };

export type CancelResult = {
  message: { order_signature_hash: string; signed_at: number };
  simulated: true;
};

export type CancelFn = (args: {
  order: CancelOrder;
  signal?: AbortSignal;
}) => Promise<CancelResult>;

export type CancelInput = {
  trackCtx: TrackContext;
  order?: CancelOrder;
  ownership?: Ownership;
  cancel?: CancelFn;
  track?: TrackFn;
};

export type Ownership = "self" | "other" | "none";

export type CancelContext = {
  trackCtx: TrackContext;
  cancel: CancelFn;
  track: TrackFn;
  order?: CancelOrder;
  ownership: Ownership;
  result?: CancelResult;
  error?: string;
};

export type CancelEvent =
  | { type: "CONNECT_WALLET" }
  | { type: "NOT_OWNER" }
  | { type: "CONFIRM" }
  | { type: "SUBMIT" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const CANCEL_EVENTS = {
  started: "mk_cancel_started",
  walletConnected: "mk_cancel_wallet_connected",
  confirmReached: "mk_cancel_confirm_reached",
  submitted: "mk_cancel_submitted",
  completed: "mk_cancel_completed",
  notOwner: "mk_cancel_not_owner",
  failed: "mk_cancel_failed",
} as const;

export const STATE_TO_SLUG = {
  reviewing: "review-listing",
  connecting: "connect-wallet",
  confirming: "confirm-cancel",
  submitting: "submit-tx",
  success: "success",
  notOwner: "not-owner",
  error: "error",
} as const;

export type CancelStateId = keyof typeof STATE_TO_SLUG;
export type CancelStepSlug = (typeof STATE_TO_SLUG)[CancelStateId];

export const FIRST_STEP_SLUG: CancelStepSlug = STATE_TO_SLUG.reviewing;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "reviewing");

export const SLUG_TO_STATE: Record<CancelStepSlug, CancelStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => CancelStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => CancelStateId = stepSlugs.toState;

export const simulateCancel: CancelFn = async ({ order, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return {
    message: {
      order_signature_hash: order.orderId,
      signed_at: Math.floor(Date.now() / 1000),
    },
    simulated: true,
  };
};

export const cancelMachine = setup({
  types: {
    context: {} as CancelContext,
    events: {} as CancelEvent,
    input: {} as CancelInput,
  },
  actors: {
    runCancel: fromPromise<CancelResult, { order: CancelOrder; cancel: CancelFn }>(
      ({ input, signal }) => input.cancel({ order: input.order, signal }),
    ),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(
        CANCEL_EVENTS.started,
        { order_id: context.order?.orderId },
        context.trackCtx,
      ),
    trackWalletConnected: ({ context }) =>
      context.track(
        CANCEL_EVENTS.walletConnected,
        { order_id: context.order?.orderId },
        context.trackCtx,
      ),
    trackConfirmReached: ({ context }) =>
      context.track(
        CANCEL_EVENTS.confirmReached,
        { order_id: context.order?.orderId, price: context.order?.price ?? undefined },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        CANCEL_EVENTS.submitted,
        { order_id: context.order?.orderId },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        CANCEL_EVENTS.completed,
        {
          order_id: context.order?.orderId,
          order_signature_hash: context.result?.message.order_signature_hash,
          stub: true,
        },
        context.trackCtx,
      ),
    trackNotOwner: ({ context }) =>
      context.track(
        CANCEL_EVENTS.notOwner,
        { order_id: context.order?.orderId, ownership: context.ownership },
        context.trackCtx,
      ),
    trackFailed: ({ context }) =>
      context.track(
        CANCEL_EVENTS.failed,
        { order_id: context.order?.orderId, error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "cancelListing",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    cancel: input.cancel ?? simulateCancel,
    track: input.track ?? defaultTrack,
    order: input.order,
    ownership: input.ownership ?? "self",
  }),
  initial: "reviewing",
  states: {
    reviewing: {
      on: {
        CONNECT_WALLET: { target: "connecting", actions: "trackStarted" },
        NOT_OWNER: { target: "notOwner", actions: "trackNotOwner" },
      },
    },
    connecting: {
      on: {
        CONFIRM: { target: "confirming", actions: "trackWalletConnected" },
        BACK: { target: "reviewing" },
      },
    },
    confirming: {
      entry: [assign({ error: undefined }), "trackConfirmReached"],
      on: {
        SUBMIT: { target: "submitting", actions: "trackSubmitted" },
        BACK: { target: "connecting" },
      },
    },
    submitting: {
      invoke: {
        id: "runCancel",
        src: "runCancel",
        input: ({ context }) => ({
          order: context.order ?? {
            orderId: "",
            owner: "",
            price: null,
            name: "",
            network: "polygon",
          },
          cancel: context.cancel,
        }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: {
          target: "error",
          actions: [
            assign({
              error: ({ event }) =>
                toErrorMessage(event.error, "cancel failed"),
            }),
            "trackFailed",
          ],
        },
      },
    },
    success: {
      type: "final",
    },
    notOwner: {
      on: {
        BACK: { target: "reviewing" },
      },
    },
    error: {
      on: {
        RETRY: { target: "submitting" },
      },
    },
  },
});

export type CancelMachine = typeof cancelMachine;

export function resolveCancelSnapshot(args: {
  step: CancelStateId;
  trackCtx: TrackContext;
  order?: CancelOrder;
  ownership?: Ownership;
  cancel?: CancelFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, order, ownership = "self", cancel, track } = args;
  if (step === "reviewing") return undefined;
  const context: CancelContext = {
    trackCtx,
    cancel: cancel ?? simulateCancel,
    track: track ?? defaultTrack,
    order,
    ownership,
  };
  return cancelMachine.resolveState({ value: step, context });
}
