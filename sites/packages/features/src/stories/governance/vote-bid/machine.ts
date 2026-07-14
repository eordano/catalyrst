import { assign, fromPromise, setup, type ErrorActorEvent } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type CastResult = { receipt: string };

export type CastFn = (args: {
  bidId: string;
  choice: string;
  attempt: number;
  signal?: AbortSignal;
}) => Promise<CastResult>;

export type BidVoteInput = {
  trackCtx: TrackContext;
  bidId: string;
  fieldSize: number;
  maxErrors?: number;
  cast?: CastFn;
  track?: TrackFn;
};

export type BidVoteContext = {
  trackCtx: TrackContext;
  bidId: string;
  fieldSize: number;
  maxErrors: number;
  cast: CastFn;
  track: TrackFn;
  choice?: string;
  attempts: number;
  result?: CastResult;
  error?: string;
};

export type BidVoteEvent =
  | { type: "ACKNOWLEDGE" }
  | { type: "SELECT_CHOICE"; choice: string }
  | { type: "CAST" }
  | { type: "BACK" }
  | { type: "RETRY" }
  | { type: "REDIRECT" };

export const BID_VOTE_EVENTS = {
  started: "gv_bid_vote_started",
  fieldReviewed: "gv_bid_vote_field_reviewed",
  choiceSelected: "gv_bid_vote_choice_selected",
  castReached: "gv_bid_vote_cast_reached",
  castFailed: "gv_bid_vote_cast_failed",
  snapshotRedirect: "gv_bid_vote_snapshot_redirect",
  completed: "gv_bid_vote_completed",
} as const;

export const BID_CHOICES = ["Yes", "No", "Abstain"] as const;
export type BidChoice = (typeof BID_CHOICES)[number];

const DEFAULT_MAX_ERRORS = 2;

export const STATE_TO_SLUG = {
  review: "review",
  choosing: "choosing",
  casting: "casting",
  error: "error",
  snapshot: "snapshot",
  completed: "completed",
} as const;

export type BidVoteStateId = keyof typeof STATE_TO_SLUG;
export type BidVoteStepSlug = (typeof STATE_TO_SLUG)[BidVoteStateId];

export const FIRST_STEP_SLUG: BidVoteStepSlug = STATE_TO_SLUG.review;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "review");

export const SLUG_TO_STATE: Record<BidVoteStepSlug, BidVoteStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => BidVoteStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => BidVoteStateId = stepSlugs.toState;

export const simulateCast: CastFn = async ({ bidId, choice, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { receipt: `sim:${bidId}:${choice.toLowerCase()}` };
};

export const bidVoteMachine = setup({
  types: {
    context: {} as BidVoteContext,
    events: {} as BidVoteEvent | ErrorActorEvent,
    input: {} as BidVoteInput,
  },
  actors: {
    runCast: fromPromise<
      CastResult,
      { bidId: string; choice: string; attempt: number; cast: CastFn }
    >(({ input, signal }) =>
      input.cast({
        bidId: input.bidId,
        choice: input.choice,
        attempt: input.attempt,
        signal,
      }),
    ),
  },
  guards: {
    hasChoice: ({ context }) => !!context.choice,
    atMaxErrors: ({ context }) => context.attempts + 1 >= context.maxErrors,
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(
        BID_VOTE_EVENTS.started,
        { bid_id: context.bidId, bids: context.fieldSize },
        context.trackCtx,
      ),
    trackFieldReviewed: ({ context }) =>
      context.track(
        BID_VOTE_EVENTS.fieldReviewed,
        { bid_id: context.bidId, bids: context.fieldSize },
        context.trackCtx,
      ),
    setChoice: assign({
      choice: ({ event }) =>
        event.type === "SELECT_CHOICE" ? event.choice : undefined,
    }),
    trackChoiceSelected: ({ context, event }) => {
      if (event.type !== "SELECT_CHOICE") return;
      context.track(
        BID_VOTE_EVENTS.choiceSelected,
        { bid_id: context.bidId, choice: event.choice },
        context.trackCtx,
      );
    },
    trackCastReached: ({ context }) =>
      context.track(
        BID_VOTE_EVENTS.castReached,
        { bid_id: context.bidId, choice: context.choice },
        context.trackCtx,
      ),
    bumpAttempts: assign({ attempts: ({ context }) => context.attempts + 1 }),
    recordError: assign({
      error: ({ event }) => {
        if (!("error" in event)) return undefined;
        return toErrorMessage(event.error, "cast failed");
      },
    }),
    trackCastFailed: ({ context }) =>
      context.track(
        BID_VOTE_EVENTS.castFailed,
        { bid_id: context.bidId, choice: context.choice, attempt: context.attempts },
        context.trackCtx,
      ),
    trackSnapshotRedirect: ({ context }) =>
      context.track(
        BID_VOTE_EVENTS.snapshotRedirect,
        { bid_id: context.bidId, attempts: context.attempts },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        BID_VOTE_EVENTS.completed,
        {
          bid_id: context.bidId,
          choice: context.choice,
          receipt: context.result?.receipt,
          stub: true,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "bidVote",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    bidId: input.bidId,
    fieldSize: input.fieldSize,
    maxErrors: input.maxErrors ?? DEFAULT_MAX_ERRORS,
    cast: input.cast ?? simulateCast,
    track: input.track ?? defaultTrack,
    attempts: 0,
  }),
  initial: "review",
  states: {
    review: {
      entry: "trackStarted",
      on: {
        ACKNOWLEDGE: { target: "choosing", actions: "trackFieldReviewed" },
      },
    },
    choosing: {
      on: {
        SELECT_CHOICE: {
          target: "choosing",
          actions: ["setChoice", "trackChoiceSelected"],
        },
        CAST: { target: "casting", guard: "hasChoice" },
        BACK: { target: "review" },
      },
    },
    casting: {
      entry: ["trackCastReached", assign({ error: undefined })],
      invoke: {
        id: "runCast",
        src: "runCast",
        input: ({ context }) => ({
          bidId: context.bidId,
          choice: context.choice ?? "Yes",
          attempt: context.attempts + 1,
          cast: context.cast,
        }),
        onDone: {
          target: "completed",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: [
          {
            target: "snapshot",
            guard: "atMaxErrors",
            actions: ["bumpAttempts", "recordError", "trackCastFailed"],
          },
          {
            target: "error",
            actions: ["bumpAttempts", "recordError", "trackCastFailed"],
          },
        ],
      },
    },
    error: {
      on: {
        RETRY: { target: "casting" },
        REDIRECT: { target: "snapshot" },
      },
    },
    snapshot: {
      entry: "trackSnapshotRedirect",
      type: "final",
    },
    completed: {
      type: "final",
    },
  },
});

export type BidVoteMachine = typeof bidVoteMachine;

export function resolveBidVoteSnapshot(args: {
  step: BidVoteStateId;
  trackCtx: TrackContext;
  bidId: string;
  fieldSize: number;
  maxErrors?: number;
  cast?: CastFn;
  track?: TrackFn;
  choice?: string;
}) {
  const { step, trackCtx, bidId, fieldSize, maxErrors, cast, track, choice = "Yes" } = args;
  if (step === "review") return undefined;
  const context: BidVoteContext = {
    trackCtx,
    bidId,
    fieldSize,
    maxErrors: maxErrors ?? DEFAULT_MAX_ERRORS,
    cast: cast ?? simulateCast,
    track: track ?? defaultTrack,
    attempts: 0,
    choice: step === "choosing" ? undefined : choice,
  };
  return bidVoteMachine.resolveState({ value: step, context });
}
