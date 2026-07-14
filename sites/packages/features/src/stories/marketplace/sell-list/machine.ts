import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  buildSellOrder,
  failClosedCreate,
  type CreateOrderFn,
  type CreateOrderResult,
  type OwnedAsset,
} from "@data/lib/catalyst/marketplace/sell";

export type { TrackFn };

export type SellInput = {
  trackCtx: TrackContext;
  assets: OwnedAsset[];
  createOrder?: CreateOrderFn;
  track?: TrackFn;
};

export type SellContext = {
  trackCtx: TrackContext;
  assets: OwnedAsset[];
  createOrder: CreateOrderFn;
  track: TrackFn;
  assetId?: string;
  priceMana?: number;
  expiresAt?: number;
  result?: CreateOrderResult;
  error?: string;
};

export type SellEvent =
  | { type: "SELECT_ASSET"; assetId: string }
  | { type: "SET_PRICE"; priceMana: number }
  | { type: "SET_EXPIRATION"; expiresAt: number }
  | { type: "APPROVE" }
  | { type: "SIGN" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const SELL_EVENTS = {
  started: "mk_sell_started",
  assetSelected: "mk_sell_asset_selected",
  priceSet: "mk_sell_price_set",
  priceInvalid: "mk_sell_price_invalid",
  expirationSet: "mk_sell_expiration_set",
  approveReached: "mk_sell_approve_reached",
  signReached: "mk_sell_sign_reached",
  confirmReached: "mk_sell_confirm_reached",
  completed: "mk_sell_completed",
  failed: "mk_sell_failed",
} as const;

export const STATE_TO_SLUG = {
  selectAsset: "select-asset",
  setPrice: "set-price",
  setExpiration: "set-expiration",
  approveNft: "approve-nft",
  signOrder: "sign-order",
  confirm: "confirm",
  success: "success",
  error: "error",
} as const;

export type SellStateId = keyof typeof STATE_TO_SLUG;
export type SellStepSlug = (typeof STATE_TO_SLUG)[SellStateId];

export const FIRST_STEP_SLUG: SellStepSlug = STATE_TO_SLUG.selectAsset;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "selectAsset");

export const SLUG_TO_STATE: Record<SellStepSlug, SellStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => SellStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => SellStateId = stepSlugs.toState;

export function isValidPrice(priceMana: number): boolean {
  return Number.isFinite(priceMana) && priceMana > 0;
}

export const sellMachine = setup({
  types: {
    context: {} as SellContext,
    events: {} as SellEvent,
    input: {} as SellInput,
  },
  actors: {
    runCreateOrder: fromPromise<
      CreateOrderResult,
      {
        assets: OwnedAsset[];
        assetId?: string;
        priceMana?: number;
        expiresAt?: number;
        createOrder: CreateOrderFn;
      }
    >(({ input, signal }) => {
      const asset = input.assets.find((a) => a.id === input.assetId);
      if (!asset) throw new Error("asset not found");
      const order = buildSellOrder({
        asset,
        priceMana: input.priceMana ?? 0,
        expiresAt: input.expiresAt ?? Date.now() + 30 * 24 * 3600 * 1000,
      });
      return input.createOrder({ order, signal });
    }),
  },
  guards: {
    priceValid: ({ event }) =>
      event.type === "SET_PRICE" && isValidPrice(event.priceMana),
  },
  actions: {
    setAsset: assign({
      assetId: ({ event }) =>
        event.type === "SELECT_ASSET" ? event.assetId : undefined,
    }),
    trackStarted: ({ context, event }) => {
      if (event.type !== "SELECT_ASSET") return;
      context.track(SELL_EVENTS.started, {}, context.trackCtx);
      context.track(
        SELL_EVENTS.assetSelected,
        { item_id: event.assetId },
        context.trackCtx,
      );
    },
    setPrice: assign({
      priceMana: ({ event }) =>
        event.type === "SET_PRICE" ? event.priceMana : undefined,
    }),
    trackPriceSet: ({ context, event }) => {
      if (event.type !== "SET_PRICE") return;
      context.track(
        SELL_EVENTS.priceSet,
        { price_mana: event.priceMana },
        context.trackCtx,
      );
    },
    trackPriceInvalid: ({ context, event }) => {
      const price = event.type === "SET_PRICE" ? event.priceMana : undefined;
      context.track(SELL_EVENTS.priceInvalid, { price_mana: price }, context.trackCtx);
    },
    setExpiration: assign({
      expiresAt: ({ event }) =>
        event.type === "SET_EXPIRATION" ? event.expiresAt : undefined,
    }),
    trackExpirationSet: ({ context, event }) => {
      if (event.type !== "SET_EXPIRATION") return;
      context.track(
        SELL_EVENTS.expirationSet,
        { expires_at: event.expiresAt },
        context.trackCtx,
      );
    },
    trackApproveReached: ({ context }) =>
      context.track(SELL_EVENTS.approveReached, { item_id: context.assetId }, context.trackCtx),
    trackSignReached: ({ context }) =>
      context.track(SELL_EVENTS.signReached, { item_id: context.assetId }, context.trackCtx),
    trackConfirmReached: ({ context }) =>
      context.track(
        SELL_EVENTS.confirmReached,
        { item_id: context.assetId, price_mana: context.priceMana },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        SELL_EVENTS.completed,
        {
          item_id: context.assetId,
          order_id: context.result?.order.id,
          approval_tx_hash: context.result?.approvalTxHash ?? null,
          price_mana: context.priceMana,
        },
        context.trackCtx,
      ),
    trackFailed: ({ context }) =>
      context.track(
        SELL_EVENTS.failed,
        { item_id: context.assetId, reason: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "sellWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    assets: input.assets,
    createOrder: input.createOrder ?? failClosedCreate,
    track: input.track ?? defaultTrack,
  }),
  initial: "selectAsset",
  states: {
    selectAsset: {
      on: {
        SELECT_ASSET: {
          target: "setPrice",
          actions: ["setAsset", "trackStarted"],
        },
      },
    },
    setPrice: {
      on: {
        SET_PRICE: [
          {
            guard: "priceValid",
            target: "setExpiration",
            actions: ["setPrice", "trackPriceSet"],
          },
          { actions: "trackPriceInvalid" },
        ],
        BACK: { target: "selectAsset" },
      },
    },
    setExpiration: {
      on: {
        SET_EXPIRATION: {
          target: "approveNft",
          actions: ["setExpiration", "trackExpirationSet"],
        },
        BACK: { target: "setPrice" },
      },
    },
    approveNft: {
      entry: "trackApproveReached",
      on: {
        APPROVE: { target: "signOrder" },
        BACK: { target: "setExpiration" },
      },
    },
    signOrder: {
      entry: "trackSignReached",
      on: {
        SIGN: { target: "confirm" },
        BACK: { target: "approveNft" },
      },
    },
    confirm: {
      entry: [assign({ error: undefined }), "trackConfirmReached"],
      invoke: {
        id: "runCreateOrder",
        src: "runCreateOrder",
        input: ({ context }) => ({
          assets: context.assets,
          assetId: context.assetId,
          priceMana: context.priceMana,
          expiresAt: context.expiresAt,
          createOrder: context.createOrder,
        }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "order create failed"),
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
        RETRY: { target: "confirm" },
      },
    },
  },
});

export type SellMachine = typeof sellMachine;

export function resolveSellSnapshot(args: {
  step: SellStateId;
  trackCtx: TrackContext;
  assets: OwnedAsset[];
  createOrder?: CreateOrderFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, assets, createOrder, track } = args;
  if (step === "selectAsset") return undefined;
  const context: SellContext = {
    trackCtx,
    assets,
    createOrder: createOrder ?? failClosedCreate,
    track: track ?? defaultTrack,
    assetId: assets[0]?.id,
    priceMana: 1000,
    expiresAt: Date.now() + 30 * 24 * 3600 * 1000,
  };
  return sellMachine.resolveState({ value: step, context });
}
