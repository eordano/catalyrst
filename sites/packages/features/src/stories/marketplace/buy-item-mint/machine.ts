import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type MintAssetInput = {
  id: string;
  name: string;
  priceMana: string | null;
  tradeId: string | null;
};

export type { TrackFn };

export type MintPhase = "connect" | "approve" | "submit";

export type MintResult = { txHash: string };

export type SimulateFn = (args: {
  phase: MintPhase;
  asset: MintAssetInput;
  signal?: AbortSignal;
}) => Promise<MintResult>;

export type MintInput = {
  asset: MintAssetInput;
  trackCtx: TrackContext;
  simulate?: SimulateFn;
  track?: TrackFn;
};

export type MintContext = {
  asset: MintAssetInput;
  trackCtx: TrackContext;
  simulate: SimulateFn;
  track: TrackFn;
  result?: MintResult;
  failedPhase?: MintPhase;
  error?: string;
};

export type MintEvent =
  | { type: "START_MINT" }
  | { type: "CONFIRM" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const MINT_EVENTS = {
  started: "mk_mint_started",
  reviewConfirmed: "mk_mint_review_confirmed",
  walletConnected: "mk_mint_wallet_connected",
  manaApproved: "mk_mint_mana_approved",
  confirmReached: "mk_mint_confirm_reached",
  submitted: "mk_mint_submitted",
  completed: "mk_mint_completed",
  failed: "mk_mint_failed",
} as const;

export const STATE_TO_SLUG = {
  review: "review",
  connecting: "connect",
  approving: "approve",
  confirming: "confirm",
  submitting: "submit",
  success: "success",
  error: "error",
} as const;

export type MintStateId = keyof typeof STATE_TO_SLUG;
export type MintStepSlug = (typeof STATE_TO_SLUG)[MintStateId];

export const FIRST_STEP_SLUG: MintStepSlug = STATE_TO_SLUG.review;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "review");

export const SLUG_TO_STATE: Record<MintStepSlug, MintStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => MintStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => MintStateId = stepSlugs.toState;

export const simulatePhase: SimulateFn = async ({ phase, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, phase === "submit" ? 500 : 300);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { txHash: `0xsimulated${phase}deadbeef` };
};

export const buyMintMachine = setup({
  types: {
    context: {} as MintContext,
    events: {} as MintEvent,
    input: {} as MintInput,
  },
  actors: {
    runPhase: fromPromise<
      MintResult,
      { phase: MintPhase; asset: MintAssetInput; simulate: SimulateFn }
    >(({ input, signal }) =>
      input.simulate({ phase: input.phase, asset: input.asset, signal }),
    ),
  },
  actions: {
    trackStarted: ({ context }) => {
      context.track(MINT_EVENTS.started, { item_id: context.asset.id }, context.trackCtx);
      context.track(
        MINT_EVENTS.reviewConfirmed,
        { item_id: context.asset.id, price_mana: context.asset.priceMana },
        context.trackCtx,
      );
    },
    trackWalletConnected: ({ context }) =>
      context.track(MINT_EVENTS.walletConnected, { item_id: context.asset.id }, context.trackCtx),
    trackManaApproved: ({ context }) =>
      context.track(MINT_EVENTS.manaApproved, { item_id: context.asset.id }, context.trackCtx),
    trackConfirmReached: ({ context }) =>
      context.track(
        MINT_EVENTS.confirmReached,
        { item_id: context.asset.id, price_mana: context.asset.priceMana },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        MINT_EVENTS.submitted,
        { item_id: context.asset.id, trade_id: context.asset.tradeId },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        MINT_EVENTS.completed,
        {
          item_id: context.asset.id,
          tx_hash: context.result?.txHash,
          trade_id: context.asset.tradeId,
          stub: true,
        },
        context.trackCtx,
      ),
    trackFailed: ({ context }) =>
      context.track(
        MINT_EVENTS.failed,
        { item_id: context.asset.id, step: context.failedPhase, error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "buyMintWizard",
  context: ({ input }) => ({
    asset: input.asset,
    trackCtx: input.trackCtx,
    simulate: input.simulate ?? simulatePhase,
    track: input.track ?? defaultTrack,
  }),
  initial: "review",
  states: {
    review: {
      on: {
        START_MINT: { target: "connecting", actions: "trackStarted" },
      },
    },

    connecting: {
      entry: assign({ error: undefined, failedPhase: undefined }),
      invoke: {
        id: "connectWallet",
        src: "runPhase",
        input: ({ context }) => ({ phase: "connect", asset: context.asset, simulate: context.simulate }),
        onDone: { target: "approving", actions: "trackWalletConnected" },
        onError: {
          target: "error",
          actions: assign({
            failedPhase: () => "connect" as const,
            error: ({ event }) =>
              toErrorMessage(event.error, "connect failed"),
          }),
        },
      },
    },

    approving: {
      invoke: {
        id: "approveMana",
        src: "runPhase",
        input: ({ context }) => ({ phase: "approve", asset: context.asset, simulate: context.simulate }),
        onDone: { target: "confirming", actions: "trackManaApproved" },
        onError: {
          target: "error",
          actions: assign({
            failedPhase: () => "approve" as const,
            error: ({ event }) =>
              toErrorMessage(event.error, "approval failed"),
          }),
        },
      },
    },

    confirming: {
      entry: "trackConfirmReached",
      on: {
        CONFIRM: { target: "submitting", actions: "trackSubmitted" },
        BACK: { target: "review" },
      },
    },

    submitting: {
      invoke: {
        id: "submitMint",
        src: "runPhase",
        input: ({ context }) => ({ phase: "submit", asset: context.asset, simulate: context.simulate }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: {
          target: "error",
          actions: assign({
            failedPhase: () => "submit" as const,
            error: ({ event }) =>
              toErrorMessage(event.error, "mint failed"),
          }),
        },
      },
    },

    success: {
      type: "final",
    },

    error: {
      entry: "trackFailed",
      on: {
        RETRY: [
          { target: "connecting", guard: ({ context }) => context.failedPhase === "connect" },
          { target: "approving", guard: ({ context }) => context.failedPhase === "approve" },
          { target: "submitting", guard: ({ context }) => context.failedPhase === "submit" },
          { target: "connecting" },
        ],
      },
    },
  },
});

export type BuyMintMachine = typeof buyMintMachine;

export function resolveMintSnapshot(args: {
  step: MintStateId;
  asset: MintAssetInput;
  trackCtx: TrackContext;
  simulate?: SimulateFn;
  track?: TrackFn;
}) {
  const { step, asset, trackCtx, simulate, track } = args;
  if (step === "review") return undefined;
  const context: MintContext = {
    asset,
    trackCtx,
    simulate: simulate ?? simulatePhase,
    track: track ?? defaultTrack,
    failedPhase: step === "error" ? "submit" : undefined,
  };
  return buyMintMachine.resolveState({ value: step, context });
}
