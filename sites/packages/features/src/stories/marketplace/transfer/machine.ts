import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import { looksLikeAddress } from "@data/lib/catalyst/marketplace/transfer";

export type TransferTarget = {
  id: string;
  name: string;
  category: string;
  rarity: string;
  network: "ethereum" | "polygon";
  image?: string | null;
};

export type { TrackFn };

export type TransferResult = { txHash: string };

export type TransferFn = (args: {
  asset: TransferTarget;
  recipient: string;
  signal?: AbortSignal;
}) => Promise<TransferResult>;

export type TransferInput = {
  trackCtx: TrackContext;
  transfer?: TransferFn;
  track?: TrackFn;
};

export type TransferContext = {
  trackCtx: TrackContext;
  transfer: TransferFn;
  track: TrackFn;
  asset?: TransferTarget;
  recipient: string;
  result?: TransferResult;
  error?: string;
};

export type TransferEvent =
  | { type: "SELECT_ASSET"; asset: TransferTarget }
  | { type: "SUBMIT_RECIPIENT"; recipient: string }
  | { type: "CONFIRM" }
  | { type: "APPROVE" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const TRANSFER_EVENTS = {
  assetSelected: "mk_transfer_asset_selected",
  started: "mk_transfer_started",
  recipientEntered: "mk_transfer_recipient_entered",
  invalidRecipient: "mk_transfer_invalid_recipient",
  reviewed: "mk_transfer_reviewed",
  confirmReached: "mk_transfer_confirm_reached",
  submitted: "mk_transfer_submitted",
  completed: "mk_transfer_completed",
} as const;

export const STATE_TO_SLUG = {
  selecting: "select-asset",
  enteringRecipient: "enter-recipient",
  reviewing: "review",
  confirming: "confirm-transfer",
  submitting: "submit-tx",
  success: "success",
  error: "error",
} as const;

export type TransferStateId = keyof typeof STATE_TO_SLUG;
export type TransferStepSlug = (typeof STATE_TO_SLUG)[TransferStateId];

export const FIRST_STEP_SLUG: TransferStepSlug = STATE_TO_SLUG.selecting;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "selecting");

export const SLUG_TO_STATE: Record<TransferStepSlug, TransferStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => TransferStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => TransferStateId = stepSlugs.toState;

export const simulateTransfer: TransferFn = async ({ asset, recipient, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const seed = `${asset.id}:${recipient}`;
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  const hex = h.toString(16).padStart(8, "0");
  const txHash = `0x${hex.repeat(8).slice(0, 64)}`;
  return { txHash };
};

export const transferMachine = setup({
  types: {
    context: {} as TransferContext,
    events: {} as TransferEvent,
    input: {} as TransferInput,
  },
  actors: {
    runTransfer: fromPromise<
      TransferResult,
      { asset: TransferTarget; recipient: string; transfer: TransferFn }
    >(({ input, signal }) =>
      input.transfer({ asset: input.asset, recipient: input.recipient, signal }),
    ),
  },
  guards: {
    recipientValid: ({ event }) =>
      event.type === "SUBMIT_RECIPIENT" && looksLikeAddress(event.recipient),
  },
  actions: {
    setAsset: assign({
      asset: ({ event }) =>
        event.type === "SELECT_ASSET" ? event.asset : undefined,
    }),
    trackAssetSelected: ({ context, event }) => {
      if (event.type !== "SELECT_ASSET") return;
      context.track(
        TRANSFER_EVENTS.assetSelected,
        { item_id: event.asset.id },
        context.trackCtx,
      );
      context.track(TRANSFER_EVENTS.started, { item_id: event.asset.id }, context.trackCtx);
    },
    setRecipient: assign({
      recipient: ({ event }) =>
        event.type === "SUBMIT_RECIPIENT" ? event.recipient.trim() : "",
    }),
    trackRecipientEntered: ({ context, event }) => {
      if (event.type !== "SUBMIT_RECIPIENT") return;
      context.track(
        TRANSFER_EVENTS.recipientEntered,
        { recipient: event.recipient.trim() },
        context.trackCtx,
      );
    },
    trackInvalidRecipient: ({ context }) =>
      context.track(TRANSFER_EVENTS.invalidRecipient, {}, context.trackCtx),
    trackReviewed: ({ context }) =>
      context.track(
        TRANSFER_EVENTS.reviewed,
        { item_id: context.asset?.id, recipient: context.recipient },
        context.trackCtx,
      ),
    trackConfirmReached: ({ context }) =>
      context.track(
        TRANSFER_EVENTS.confirmReached,
        { item_id: context.asset?.id },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        TRANSFER_EVENTS.submitted,
        { item_id: context.asset?.id, recipient: context.recipient, stub: true },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        TRANSFER_EVENTS.completed,
        {
          item_id: context.asset?.id,
          recipient: context.recipient,
          tx_hash: context.result?.txHash,
          stub: true,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "transferWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    transfer: input.transfer ?? simulateTransfer,
    track: input.track ?? defaultTrack,
    recipient: "",
  }),
  initial: "selecting",
  states: {
    selecting: {
      on: {
        SELECT_ASSET: {
          target: "enteringRecipient",
          actions: ["setAsset", "trackAssetSelected"],
        },
      },
    },
    enteringRecipient: {
      on: {
        SUBMIT_RECIPIENT: [
          {
            guard: "recipientValid",
            target: "reviewing",
            actions: ["setRecipient", "trackRecipientEntered"],
          },
          { actions: "trackInvalidRecipient" },
        ],
        BACK: { target: "selecting" },
      },
    },
    reviewing: {
      entry: "trackReviewed",
      on: {
        CONFIRM: { target: "confirming" },
        BACK: { target: "enteringRecipient" },
      },
    },
    confirming: {
      entry: "trackConfirmReached",
      on: {
        APPROVE: { target: "submitting" },
        BACK: { target: "reviewing" },
      },
    },
    submitting: {
      entry: [assign({ error: undefined }), "trackSubmitted"],
      invoke: {
        id: "runTransfer",
        src: "runTransfer",
        input: ({ context }) => ({
          asset: context.asset ?? {
            id: "",
            name: "",
            category: "wearable",
            rarity: "common",
            network: "polygon" as const,
          },
          recipient: context.recipient,
          transfer: context.transfer,
        }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "transfer failed"),
          }),
        },
      },
    },
    success: {
      type: "final",
    },
    error: {
      on: {
        RETRY: { target: "submitting" },
        BACK: { target: "reviewing" },
      },
    },
  },
});

export type TransferMachine = typeof transferMachine;

export function resolveTransferSnapshot(args: {
  step: TransferStateId;
  trackCtx: TrackContext;
  transfer?: TransferFn;
  track?: TrackFn;
  asset?: TransferTarget;
  recipient?: string;
}) {
  const { step, trackCtx, transfer, track, asset, recipient } = args;
  if (step === "selecting") return undefined;
  const seededAsset: TransferTarget =
    asset ?? {
      id: "0x0-0",
      name: "Sample Wearable",
      category: "wearable",
      rarity: "rare",
      network: "polygon",
    };
  const context: TransferContext = {
    trackCtx,
    transfer: transfer ?? simulateTransfer,
    track: track ?? defaultTrack,
    asset: seededAsset,
    recipient: recipient ?? "0x1d9aa2025b67f0f21d1603ce521bda7869098f8a",
  };
  return transferMachine.resolveState({ value: step, context });
}
