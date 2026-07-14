import { delay, makeStepSlugs } from "@core/lib/stories/index";
import { toErrorMessage } from "@core/lib/errors";
import { assign, fromPromise, setup } from "xstate";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type BuyListing = {
  assetId: string;
  contractAddress: string;
  tokenId: string;
  priceMana: string;
  priceWei: string;
  network: "ethereum" | "polygon";
  marketplaceAddress: string | null;
  chainId: number | null;
  seller: string;
};

export type TradeResult = { txHash: string };

export type SimFn = (args: {
  listing: BuyListing;
  signal?: AbortSignal;
}) => Promise<TradeResult>;

export type BuyInput = {
  listing: BuyListing;
  trackCtx: TrackContext;
  connect?: SimFn;
  approve?: SimFn;
  commit?: SimFn;
  track?: TrackFn;
};

export type BuyContext = {
  listing: BuyListing;
  trackCtx: TrackContext;
  connect: SimFn;
  approve: SimFn;
  commit: SimFn;
  track: TrackFn;
  result?: TradeResult;
  error?: string;
};

export type BuyEvent =
  | { type: "START" }
  | { type: "CONFIRM" }
  | { type: "CANCEL" }
  | { type: "RETRY" };

export const BUY_EVENTS = {
  started: "mk_buy_started",
  walletConnected: "mk_buy_wallet_connected",
  manaApproved: "mk_buy_mana_approved",
  confirmReached: "mk_buy_confirm_reached",
  completed: "mk_buy_completed",
  failed: "mk_buy_failed",
} as const;

export const STATE_TO_SLUG = {
  review: "review",
  connecting: "connect-wallet",
  approving: "approve-mana",
  confirming: "confirm-purchase",
  submitting: "submit-tx",
  success: "success",
  error: "error",
} as const;

export type BuyStateId = keyof typeof STATE_TO_SLUG;
export type BuyStepSlug = (typeof STATE_TO_SLUG)[BuyStateId];

export const FIRST_STEP_SLUG: BuyStepSlug = STATE_TO_SLUG.review;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "review");

export const SLUG_TO_STATE: Record<BuyStepSlug, BuyStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => BuyStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => BuyStateId = stepSlugs.toState;

export const simulateConnect: SimFn = async ({ signal }) => {
  await delay(300, signal);
  return { txHash: "" };
};

export const simulateApprove: SimFn = async ({ signal }) => {
  await delay(300, signal);
  return { txHash: "" };
};

export const simulateTradeCommit: SimFn = async ({ listing, signal }) => {
  await delay(400, signal);
  const seed = `${listing.contractAddress}-${listing.tokenId}`;
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  const txHash = "0x" + h.toString(16).padStart(8, "0").repeat(8).slice(0, 64);
  return { txHash };
};

export const buyMachine = setup({
  types: {
    context: {} as BuyContext,
    events: {} as BuyEvent,
    input: {} as BuyInput,
  },
  actors: {
    runConnect: fromPromise<TradeResult, { listing: BuyListing; sim: SimFn }>(
      ({ input, signal }) => input.sim({ listing: input.listing, signal }),
    ),
    runApprove: fromPromise<TradeResult, { listing: BuyListing; sim: SimFn }>(
      ({ input, signal }) => input.sim({ listing: input.listing, signal }),
    ),
    runCommit: fromPromise<TradeResult, { listing: BuyListing; sim: SimFn }>(
      ({ input, signal }) => input.sim({ listing: input.listing, signal }),
    ),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(
        BUY_EVENTS.started,
        { asset_id: context.listing.assetId, price_mana: context.listing.priceMana },
        context.trackCtx,
      ),
    trackWalletConnected: ({ context }) =>
      context.track(
        BUY_EVENTS.walletConnected,
        { asset_id: context.listing.assetId },
        context.trackCtx,
      ),
    trackManaApproved: ({ context }) =>
      context.track(
        BUY_EVENTS.manaApproved,
        { asset_id: context.listing.assetId, price_mana: context.listing.priceMana },
        context.trackCtx,
      ),
    trackConfirmReached: ({ context }) =>
      context.track(
        BUY_EVENTS.confirmReached,
        { asset_id: context.listing.assetId, price_wei: context.listing.priceWei },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        BUY_EVENTS.completed,
        {
          asset_id: context.listing.assetId,
          tx_hash: context.result?.txHash,
          stub: true,
        },
        context.trackCtx,
      ),
    trackFailed: ({ context }) =>
      context.track(
        BUY_EVENTS.failed,
        { asset_id: context.listing.assetId, error: context.error },
        context.trackCtx,
      ),
    setError: assign({
      error: ({ event }) => {
        const e = event as { error?: unknown };
        return toErrorMessage(e.error, "buy failed");
      },
    }),
    clearError: assign({ error: undefined }),
  },
}).createMachine({
  id: "buyWizard",
  context: ({ input }) => ({
    listing: input.listing,
    trackCtx: input.trackCtx,
    connect: input.connect ?? simulateConnect,
    approve: input.approve ?? simulateApprove,
    commit: input.commit ?? simulateTradeCommit,
    track: input.track ?? defaultTrack,
  }),
  initial: "review",
  states: {
    review: {
      on: {
        START: { target: "connecting", actions: "trackStarted" },
        CANCEL: { target: "review" },
      },
    },
    connecting: {
      entry: "clearError",
      invoke: {
        id: "runConnect",
        src: "runConnect",
        input: ({ context }) => ({ listing: context.listing, sim: context.connect }),
        onDone: { target: "approving", actions: "trackWalletConnected" },
        onError: { target: "error", actions: ["setError", "trackFailed"] },
      },
    },
    approving: {
      entry: "clearError",
      invoke: {
        id: "runApprove",
        src: "runApprove",
        input: ({ context }) => ({ listing: context.listing, sim: context.approve }),
        onDone: { target: "confirming", actions: "trackManaApproved" },
        onError: { target: "error", actions: ["setError", "trackFailed"] },
      },
    },
    confirming: {
      on: {
        CONFIRM: { target: "submitting" },
        CANCEL: { target: "review" },
      },
    },
    submitting: {
      entry: ["clearError", "trackConfirmReached"],
      invoke: {
        id: "runCommit",
        src: "runCommit",
        input: ({ context }) => ({ listing: context.listing, sim: context.commit }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: { target: "error", actions: ["setError", "trackFailed"] },
      },
    },
    success: {
      type: "final",
    },
    error: {
      on: {
        RETRY: { target: "connecting" },
      },
    },
  },
});

export type BuyMachine = typeof buyMachine;

export function resolveBuySnapshot(args: {
  step: BuyStateId;
  listing: BuyListing;
  trackCtx: TrackContext;
  connect?: SimFn;
  approve?: SimFn;
  commit?: SimFn;
  track?: TrackFn;
}) {
  const { step, listing, trackCtx, connect, approve, commit, track } = args;
  if (step === "review") return undefined;
  const context: BuyContext = {
    listing,
    trackCtx,
    connect: connect ?? simulateConnect,
    approve: approve ?? simulateApprove,
    commit: commit ?? simulateTradeCommit,
    track: track ?? defaultTrack,
  };
  return buyMachine.resolveState({ value: step, context });
}
