import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type BidPhase = "approve" | "sign" | "place";

export type ChainFn = (args: {
  phase: BidPhase;
  price?: number;
  expiration?: string;
  signal?: AbortSignal;
}) => Promise<void>;

export type BidInput = {
  trackCtx: TrackContext;
  manaBalance?: number;
  chain?: ChainFn;
  track?: TrackFn;
};

export type BidContext = {
  trackCtx: TrackContext;
  manaBalance: number;
  chain: ChainFn;
  track: TrackFn;
  price?: number;
  expiration?: string;
  error?: string;
};

export type BidEvent =
  | { type: "REVIEW" }
  | { type: "SET_AMOUNT"; price: number }
  | { type: "SET_EXPIRATION"; expiration: string }
  | { type: "BACK" }
  | { type: "RETRY" };

export const BID_EVENTS = {
  started: "mk_bid_started",
  amountSet: "mk_bid_amount_set",
  insufficientMana: "mk_bid_insufficient_mana",
  expirationSet: "mk_bid_expiration_set",
  manaApproved: "mk_bid_mana_approved",
  signReached: "mk_bid_sign_reached",
  signed: "mk_bid_signed",
  confirmed: "mk_bid_confirmed",
  completed: "mk_bid_completed",
  failed: "mk_bid_failed",
} as const;

export const STATE_TO_SLUG = {
  asset: "asset",
  setAmount: "set-amount",
  setExpiration: "set-expiration",
  approveMana: "approve-mana",
  signing: "sign-bid",
  confirming: "confirm",
  success: "success",
  insufficient: "insufficient",
  failed: "failed",
} as const;

export type BidStateId = keyof typeof STATE_TO_SLUG;
export type BidStepSlug = (typeof STATE_TO_SLUG)[BidStateId];

export const FIRST_STEP_SLUG: BidStepSlug = STATE_TO_SLUG.asset;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "asset");

export const SLUG_TO_STATE: Record<BidStepSlug, BidStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => BidStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => BidStateId = stepSlugs.toState;

export const simulateChain: ChainFn = async ({ signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 350);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
};

export const bidMachine = setup({
  types: {
    context: {} as BidContext,
    events: {} as BidEvent,
    input: {} as BidInput,
  },
  actors: {
    runChain: fromPromise<
      void,
      { phase: BidPhase; price?: number; expiration?: string; chain: ChainFn }
    >(({ input, signal }) =>
      input.chain({
        phase: input.phase,
        price: input.price,
        expiration: input.expiration,
        signal,
      }),
    ),
  },
  guards: {
    canAfford: ({ context, event }) =>
      event.type === "SET_AMOUNT" && event.price <= context.manaBalance,
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(BID_EVENTS.started, {}, context.trackCtx),
    setAmount: assign({
      price: ({ event }) =>
        event.type === "SET_AMOUNT" ? event.price : undefined,
    }),
    trackAmountSet: ({ context, event }) => {
      if (event.type !== "SET_AMOUNT") return;
      context.track(BID_EVENTS.amountSet, { price: event.price }, context.trackCtx);
    },
    trackInsufficient: ({ context, event }) => {
      if (event.type !== "SET_AMOUNT") return;
      context.track(
        BID_EVENTS.insufficientMana,
        { price: event.price, balance: context.manaBalance },
        context.trackCtx,
      );
    },
    setExpiration: assign({
      expiration: ({ event }) =>
        event.type === "SET_EXPIRATION" ? event.expiration : undefined,
    }),
    trackExpirationSet: ({ context, event }) => {
      if (event.type !== "SET_EXPIRATION") return;
      context.track(
        BID_EVENTS.expirationSet,
        { expiration: event.expiration },
        context.trackCtx,
      );
    },
    trackManaApproved: ({ context }) =>
      context.track(BID_EVENTS.manaApproved, { price: context.price }, context.trackCtx),
    trackSignReached: ({ context }) =>
      context.track(BID_EVENTS.signReached, { price: context.price }, context.trackCtx),
    trackSigned: ({ context }) =>
      context.track(BID_EVENTS.signed, { price: context.price }, context.trackCtx),
    trackConfirmed: ({ context }) =>
      context.track(BID_EVENTS.confirmed, { price: context.price }, context.trackCtx),
    trackCompleted: ({ context }) =>
      context.track(
        BID_EVENTS.completed,
        { price: context.price, expiration: context.expiration, stub: true },
        context.trackCtx,
      ),
    recordError: assign({
      error: ({ event }) => {
        const e = (event as { error?: unknown }).error;
        return toErrorMessage(e, "bid failed");
      },
    }),
    trackFailed: ({ context }) =>
      context.track(BID_EVENTS.failed, { where: context.error }, context.trackCtx),
  },
}).createMachine({
  id: "bidWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    manaBalance: input.manaBalance ?? 50000,
    chain: input.chain ?? simulateChain,
    track: input.track ?? defaultTrack,
  }),
  initial: "asset",
  states: {
    asset: {
      on: {
        REVIEW: { target: "setAmount", actions: "trackStarted" },
      },
    },
    setAmount: {
      on: {
        SET_AMOUNT: [
          {
            guard: "canAfford",
            target: "setExpiration",
            actions: ["setAmount", "trackAmountSet"],
          },
          {
            target: "insufficient",
            actions: ["setAmount", "trackInsufficient"],
          },
        ],
      },
    },
    insufficient: {
      on: {
        BACK: { target: "setAmount" },
      },
    },
    setExpiration: {
      on: {
        SET_EXPIRATION: {
          target: "approveMana",
          actions: ["setExpiration", "trackExpirationSet"],
        },
        BACK: { target: "setAmount" },
      },
    },
    approveMana: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "approve",
        src: "runChain",
        input: ({ context }) => ({
          phase: "approve" as const,
          price: context.price,
          expiration: context.expiration,
          chain: context.chain,
        }),
        onDone: { target: "signing", actions: "trackManaApproved" },
        onError: { target: "failed", actions: ["recordError", "trackFailed"] },
      },
      on: {
        BACK: { target: "setExpiration" },
      },
    },
    signing: {
      entry: "trackSignReached",
      invoke: {
        id: "sign",
        src: "runChain",
        input: ({ context }) => ({
          phase: "sign" as const,
          price: context.price,
          expiration: context.expiration,
          chain: context.chain,
        }),
        onDone: { target: "confirming", actions: "trackSigned" },
        onError: { target: "failed", actions: ["recordError", "trackFailed"] },
      },
    },
    confirming: {
      entry: "trackConfirmed",
      invoke: {
        id: "place",
        src: "runChain",
        input: ({ context }) => ({
          phase: "place" as const,
          price: context.price,
          expiration: context.expiration,
          chain: context.chain,
        }),
        onDone: { target: "success", actions: "trackCompleted" },
        onError: { target: "failed", actions: ["recordError", "trackFailed"] },
      },
    },
    success: {
      type: "final",
    },
    failed: {
      on: {
        RETRY: { target: "approveMana" },
      },
    },
  },
});

export type BidMachine = typeof bidMachine;

export function resolveBidSnapshot(args: {
  step: BidStateId;
  trackCtx: TrackContext;
  manaBalance?: number;
  chain?: ChainFn;
  track?: TrackFn;
  price?: number;
  expiration?: string;
}) {
  const { step, trackCtx, manaBalance, chain, track, price, expiration } = args;
  if (step === "asset") return undefined;
  const context: BidContext = {
    trackCtx,
    manaBalance: manaBalance ?? 50000,
    chain: chain ?? simulateChain,
    track: track ?? defaultTrack,
    price: price ?? 1000,
    expiration: expiration ?? "2026-07-20",
  };
  return bidMachine.resolveState({ value: step, context });
}
