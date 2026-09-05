import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import {
  simulateAccept,
  type AcceptFn,
  type AcceptResult,
  type Bid,
} from "@data/lib/catalyst/marketplace/bids";
export type { AcceptFn };
import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type ApproveFn = (args: { bid: Bid; signal?: AbortSignal }) => Promise<void>;

export type AcceptInput = {
  trackCtx: TrackContext;
  bid: Bid;
  approve?: ApproveFn;
  accept?: AcceptFn;
  track?: TrackFn;
};

export type AcceptContext = {
  trackCtx: TrackContext;
  bid: Bid;
  approve: ApproveFn;
  accept: AcceptFn;
  track: TrackFn;
  result?: AcceptResult;
  error?: string;
};

export type AcceptEvent =
  | { type: "ACCEPT" }
  | { type: "REJECT" }
  | { type: "CONNECT" }
  | { type: "CONFIRM" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const ACCEPT_EVENTS = {
  started: "mk_accept_bid_started",
  rejected: "mk_accept_bid_rejected",
  walletConnected: "mk_accept_bid_wallet_connected",
  nftApproved: "mk_accept_bid_nft_approved",
  confirmReached: "mk_accept_bid_confirm_reached",
  submitted: "mk_accept_bid_submitted",
  completed: "mk_accept_bid_completed",
} as const;

export const STATE_TO_SLUG = {
  reviewBid: "review-bid",
  connectWallet: "connect-wallet",
  approveNft: "approve-nft",
  confirmAccept: "confirm-accept",
  submitTx: "submit-tx",
  success: "success",
  rejected: "rejected",
  error: "error",
} as const;

export type AcceptStateId = keyof typeof STATE_TO_SLUG;
export type AcceptStepSlug = (typeof STATE_TO_SLUG)[AcceptStateId];

export const FIRST_STEP_SLUG: AcceptStepSlug = STATE_TO_SLUG.reviewBid;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "reviewBid");

export const SLUG_TO_STATE: Record<AcceptStepSlug, AcceptStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => AcceptStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => AcceptStateId = stepSlugs.toState;

export const simulateApprove: ApproveFn = async ({ signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 350);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
};

export const acceptBidMachine = setup({
  types: {
    context: {} as AcceptContext,
    events: {} as AcceptEvent,
    input: {} as AcceptInput,
  },
  actors: {
    runApprove: fromPromise<void, { bid: Bid; approve: ApproveFn }>(
      ({ input, signal }) => input.approve({ bid: input.bid, signal }),
    ),
    runAccept: fromPromise<AcceptResult, { bid: Bid; accept: AcceptFn }>(
      ({ input, signal }) => input.accept({ bid: input.bid, signal }),
    ),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(
        ACCEPT_EVENTS.started,
        { bid_id: context.bid.id, price: context.bid.priceMana },
        context.trackCtx,
      ),
    trackRejected: ({ context }) =>
      context.track(
        ACCEPT_EVENTS.rejected,
        { bid_id: context.bid.id },
        context.trackCtx,
      ),
    trackWalletConnected: ({ context }) =>
      context.track(
        ACCEPT_EVENTS.walletConnected,
        { bid_id: context.bid.id },
        context.trackCtx,
      ),
    trackNftApproved: ({ context }) =>
      context.track(
        ACCEPT_EVENTS.nftApproved,
        { bid_id: context.bid.id, simulated: true },
        context.trackCtx,
      ),
    trackConfirmReached: ({ context }) =>
      context.track(
        ACCEPT_EVENTS.confirmReached,
        { bid_id: context.bid.id, price: context.bid.priceMana },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        ACCEPT_EVENTS.submitted,
        { bid_id: context.bid.id },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        ACCEPT_EVENTS.completed,
        { bid_id: context.bid.id, tx_hash: context.result?.txHash, stub: true },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "acceptBidWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    bid: input.bid,
    approve: input.approve ?? simulateApprove,
    accept: input.accept ?? simulateAccept,
    track: input.track ?? defaultTrack,
  }),
  initial: "reviewBid",
  states: {
    reviewBid: {
      on: {
        ACCEPT: { target: "connectWallet", actions: "trackStarted" },
        REJECT: { target: "rejected", actions: "trackRejected" },
      },
    },
    connectWallet: {
      on: {
        CONNECT: { target: "approveNft", actions: "trackWalletConnected" },
        BACK: { target: "reviewBid" },
      },
    },
    approveNft: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runApprove",
        src: "runApprove",
        input: ({ context }) => ({ bid: context.bid, approve: context.approve }),
        onDone: { target: "confirmAccept" },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "approval failed"),
          }),
        },
      },
    },
    confirmAccept: {
      entry: ["trackNftApproved", "trackConfirmReached"],
      on: {
        CONFIRM: { target: "submitTx", actions: "trackSubmitted" },
        BACK: { target: "connectWallet" },
      },
    },
    submitTx: {
      invoke: {
        id: "runAccept",
        src: "runAccept",
        input: ({ context }) => ({ bid: context.bid, accept: context.accept }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "accept failed"),
          }),
        },
      },
    },
    success: {
      type: "final",
    },
    rejected: {
      on: {
        BACK: { target: "reviewBid" },
      },
    },
    error: {
      on: {
        RETRY: { target: "approveNft" },
      },
    },
  },
});

export type AcceptBidMachine = typeof acceptBidMachine;

export function resolveAcceptSnapshot(args: {
  step: AcceptStateId;
  trackCtx: TrackContext;
  bid: Bid;
  approve?: ApproveFn;
  accept?: AcceptFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, bid, approve, accept, track } = args;
  if (step === "reviewBid") return undefined;
  const context: AcceptContext = {
    trackCtx,
    bid,
    approve: approve ?? simulateApprove,
    accept: accept ?? simulateAccept,
    track: track ?? defaultTrack,
  };
  return acceptBidMachine.resolveState({ value: step, context });
}
