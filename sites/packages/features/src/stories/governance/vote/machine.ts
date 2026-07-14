import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export const MAX_ERRORS_BEFORE_SNAPSHOT = 3;

export type CastResult = {
  receipt: string;
};

export type CastFn = (args: {
  proposalId: string;
  choice: string;
  reason?: string;
  signal?: AbortSignal;
}) => Promise<CastResult>;

export type { TrackFn };

export type VoteInput = {
  proposalId: string;
  choice: string;
  totalVp: string;
  trackCtx: TrackContext;
  guided: boolean;
  castVote?: CastFn;
  track?: TrackFn;
};

export type VoteContext = {
  proposalId: string;
  choice: string;
  totalVp: string;
  trackCtx: TrackContext;
  guided: boolean;
  castVote: CastFn;
  track: TrackFn;
  reason: string;
  attempts: number;
  receipt?: string;
  error?: string;
};

export type VoteEvent =
  | { type: "START" }
  | { type: "REASON"; reason: string }
  | { type: "CAST" }
  | { type: "CANCEL" }
  | { type: "RETRY" }
  | { type: "SUBSCRIBE" }
  | { type: "DISMISS" };

export const VOTE_EVENTS = {
  started: "gv_vote_started",
  reasoned: "gv_vote_reasoned",
  completed: "gv_vote_completed",
  snapshotRedirect: "gv_vote_snapshot_redirect",
} as const;

export const simulateCast: CastFn = async ({ proposalId, choice }) => {
  return { receipt: `stub:${proposalId}:${choice.toLowerCase()}` };
};

export const voteMachine = setup({
  types: {
    context: {} as VoteContext,
    events: {} as VoteEvent,
    input: {} as VoteInput,
  },
  actors: {
    castVote: fromPromise<
      CastResult,
      { proposalId: string; choice: string; reason: string; castVote: CastFn }
    >(({ input, signal }) =>
      input.castVote({
        proposalId: input.proposalId,
        choice: input.choice,
        reason: input.reason || undefined,
        signal,
      }),
    ),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(
        VOTE_EVENTS.started,
        {
          proposal_id: context.proposalId,
          choice: context.choice,
          guided: context.guided,
        },
        context.trackCtx,
      ),
    trackReasoned: ({ context }) =>
      context.track(
        VOTE_EVENTS.reasoned,
        { proposal_id: context.proposalId },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        VOTE_EVENTS.completed,
        {
          proposal_id: context.proposalId,
          choice: context.choice,
          receipt: context.receipt,
        },
        context.trackCtx,
      ),
    trackSnapshotRedirect: ({ context }) =>
      context.track(
        VOTE_EVENTS.snapshotRedirect,
        { proposal_id: context.proposalId, attempts: context.attempts },
        context.trackCtx,
      ),
  },
  guards: {
    isGuided: ({ context }) => context.guided === true,
    exhausted: ({ context }) =>
      context.guided === true && context.attempts >= MAX_ERRORS_BEFORE_SNAPSHOT,
  },
}).createMachine({
  id: "governanceVote",
  context: ({ input }) => ({
    proposalId: input.proposalId,
    choice: input.choice,
    totalVp: input.totalVp,
    trackCtx: input.trackCtx,
    guided: input.guided,
    castVote: input.castVote ?? simulateCast,
    track: input.track ?? defaultTrack,
    reason: "",
    attempts: 0,
  }),
  initial: "choosing",
  states: {
    choosing: {
      on: {
        START: [
          {
            target: "reasoning",
            guard: "isGuided",
            actions: "trackStarted",
          },
          {
            target: "casting",
            actions: "trackStarted",
          },
        ],
      },
    },
    reasoning: {
      on: {
        REASON: {
          actions: [
            assign({ reason: ({ event }) => event.reason }),
            "trackReasoned",
          ],
        },
        CAST: { target: "casting" },
        CANCEL: { target: "choosing" },
      },
    },
    casting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "castVote",
        src: "castVote",
        input: ({ context }) => ({
          proposalId: context.proposalId,
          choice: context.choice,
          reason: context.reason,
          castVote: context.castVote,
        }),
        onDone: {
          target: "registered",
          actions: [
            assign({ receipt: ({ event }) => event.output?.receipt }),
            "trackCompleted",
          ],
        },
        onError: {
          target: "castError",
          actions: assign({
            attempts: ({ context }) => context.attempts + 1,
            error: ({ event }) =>
              toErrorMessage(event.error, "cast failed"),
          }),
        },
      },
    },
    castError: {
      always: [{ guard: "exhausted", target: "snapshotFallback" }],
      on: {
        RETRY: { target: "casting" },
        CANCEL: { target: "choosing" },
      },
    },
    snapshotFallback: {
      entry: "trackSnapshotRedirect",
      type: "final",
    },
    registered: {
      on: {
        SUBSCRIBE: { target: "done" },
        DISMISS: { target: "done" },
      },
    },
    done: {
      type: "final",
    },
  },
});

export type VoteMachine = typeof voteMachine;
