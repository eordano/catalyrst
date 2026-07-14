import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type Rarity =
  | "unique"
  | "mythic"
  | "exotic"
  | "legendary"
  | "epic"
  | "rare"
  | "uncommon"
  | "common";

export type PublishItem = {
  id: string;
  name: string;
  rarity: Rarity;
  kind: "wearable" | "emote";
};

export type PublishCollection = {
  id: string;
  name: string;
  items: PublishItem[];
};

export type FeeLine = {
  rarity: Rarity;
  count: number;
  manaPerItem: number;
  mana: number;
};

export type FeeBreakdown = {
  lines: FeeLine[];
  itemCount: number;
  manaPerItem: number;
  totalMana: number;
};

export type PublishResult = { txHash: string };

export type PublishFn = (args: {
  collection: PublishCollection;
  totalMana: number;
  signal?: AbortSignal;
}) => Promise<PublishResult>;

export type PublishInput = {
  collection: PublishCollection;
  trackCtx: TrackContext;
  manaPerItem?: number;
  publish?: PublishFn;
  track?: TrackFn;
};

export type PublishContext = {
  collection: PublishCollection;
  trackCtx: TrackContext;
  manaPerItem: number;
  fee: FeeBreakdown;
  publish: PublishFn;
  track: TrackFn;
  result?: PublishResult;
  error?: string;
};

export type PublishEvent =
  | { type: "NEXT" }
  | { type: "ACCEPT" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const PUBLISH_EVENTS = {
  started: "bd_publish_collection_started",
  costShown: "bd_publish_collection_cost_shown",
  termsAccepted: "bd_publish_collection_terms_accepted",
  feePaid: "bd_publish_fee_paid",
  submitted: "bd_publish_submitted",
} as const;

export const DEFAULT_MANA_PER_ITEM = 100;

const RARITY_ORDER: Rarity[] = [
  "unique",
  "mythic",
  "exotic",
  "legendary",
  "epic",
  "rare",
  "uncommon",
  "common",
];

export function computeFee(
  items: PublishItem[],
  manaPerItem: number = DEFAULT_MANA_PER_ITEM,
): FeeBreakdown {
  const counts = new Map<Rarity, number>();
  for (const it of items) counts.set(it.rarity, (counts.get(it.rarity) ?? 0) + 1);

  const lines: FeeLine[] = [];
  for (const rarity of RARITY_ORDER) {
    const count = counts.get(rarity) ?? 0;
    if (count === 0) continue;
    lines.push({ rarity, count, manaPerItem, mana: count * manaPerItem });
  }

  const itemCount = items.length;
  return { lines, itemCount, manaPerItem, totalMana: itemCount * manaPerItem };
}

export const STATE_TO_SLUG = {
  summary: "summary",
  cost: "cost",
  terms: "terms",
  pay: "pay",
  submitted: "submitted",
  error: "error",
  blocked: "blocked",
} as const;

export type PublishStateId = keyof typeof STATE_TO_SLUG;
export type PublishStepSlug = (typeof STATE_TO_SLUG)[PublishStateId];

export const FIRST_STEP_SLUG: PublishStepSlug = STATE_TO_SLUG.summary;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "summary");

export const SLUG_TO_STATE: Record<PublishStepSlug, PublishStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => PublishStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => PublishStateId = stepSlugs.toState;

export const simulatePublish: PublishFn = async ({ collection, totalMana, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const seed = `${collection.id}:${totalMana}`;
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (Math.imul(h, 31) + seed.charCodeAt(i)) >>> 0;
  const txHash = `0xsim${h.toString(16).padStart(8, "0")}`;
  return { txHash };
};

export const publishMachine = setup({
  types: {
    context: {} as PublishContext,
    events: {} as PublishEvent,
    input: {} as PublishInput,
  },
  actors: {
    runPublish: fromPromise<
      PublishResult,
      { collection: PublishCollection; totalMana: number; publish: PublishFn }
    >(({ input, signal }) =>
      input.publish({
        collection: input.collection,
        totalMana: input.totalMana,
        signal,
      }),
    ),
  },
  guards: {
    hasItems: ({ context }) => context.collection.items.length > 0,
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(
        PUBLISH_EVENTS.started,
        { id: context.collection.id, itemCount: context.collection.items.length },
        context.trackCtx,
      ),
    trackCostShown: ({ context }) =>
      context.track(
        PUBLISH_EVENTS.costShown,
        { mana: context.fee.totalMana },
        context.trackCtx,
      ),
    trackTermsAccepted: ({ context }) =>
      context.track(PUBLISH_EVENTS.termsAccepted, {}, context.trackCtx),
    trackFeePaid: ({ context }) =>
      context.track(
        PUBLISH_EVENTS.feePaid,
        { mana: context.fee.totalMana, tx_hash: context.result?.txHash, simulated: true },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        PUBLISH_EVENTS.submitted,
        {
          id: context.collection.id,
          itemCount: context.collection.items.length,
          mana: context.fee.totalMana,
          stub: true,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "creatorWearablePublishCollection",
  context: ({ input }) => {
    const manaPerItem = input.manaPerItem ?? DEFAULT_MANA_PER_ITEM;
    return {
      collection: input.collection,
      trackCtx: input.trackCtx,
      manaPerItem,
      fee: computeFee(input.collection.items, manaPerItem),
      publish: input.publish ?? simulatePublish,
      track: input.track ?? defaultTrack,
    };
  },
  initial: "decide",
  states: {
    decide: {
      always: [
        { guard: "hasItems", target: "summary" },
        { target: "blocked" },
      ],
    },
    summary: {
      entry: "trackStarted",
      on: {
        NEXT: { target: "cost" },
      },
    },
    cost: {
      entry: "trackCostShown",
      on: {
        NEXT: { target: "terms" },
        BACK: { target: "summary" },
      },
    },
    terms: {
      on: {
        ACCEPT: [
          { guard: "hasItems", target: "pay", actions: "trackTermsAccepted" },
          { target: "blocked" },
        ],
        BACK: { target: "cost" },
      },
    },
    pay: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runPublish",
        src: "runPublish",
        input: ({ context }) => ({
          collection: context.collection,
          totalMana: context.fee.totalMana,
          publish: context.publish,
        }),
        onDone: {
          target: "submitted",
          actions: [assign({ result: ({ event }) => event.output }), "trackFeePaid"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "publish failed"),
          }),
        },
      },
    },
    submitted: {
      entry: "trackSubmitted",
      type: "final",
    },
    error: {
      on: {
        RETRY: [
          { guard: "hasItems", target: "pay" },
          { target: "blocked" },
        ],
        BACK: { target: "terms" },
      },
    },
    blocked: {
      type: "final",
    },
  },
});

export type PublishMachine = typeof publishMachine;

export function resolvePublishSnapshot(args: {
  step: PublishStateId;
  collection: PublishCollection;
  trackCtx: TrackContext;
  manaPerItem?: number;
  publish?: PublishFn;
  track?: TrackFn;
}) {
  const { step, collection, trackCtx, manaPerItem, publish, track } = args;
  if (step === "summary") return undefined;
  const itemIndependent = step === "cost" || step === "terms" || step === "error";
  if (!itemIndependent && collection.items.length === 0) return undefined;

  const mpi = manaPerItem ?? DEFAULT_MANA_PER_ITEM;
  const context: PublishContext = {
    collection,
    trackCtx,
    manaPerItem: mpi,
    fee: computeFee(collection.items, mpi),
    publish: publish ?? simulatePublish,
    track: track ?? defaultTrack,
  };
  return publishMachine.resolveState({ value: step, context });
}
