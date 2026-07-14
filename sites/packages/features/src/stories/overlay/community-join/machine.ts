import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  simulateCommit,
  type CommitFn,
  type CommitResult,
  type JoinAction,
} from "@data/lib/catalyst/overlay/community-join";

export type { TrackFn };

export type CommunityJoinInput = {
  trackCtx: TrackContext;
  commit?: CommitFn;
  track?: TrackFn;
  communityId?: string;
  action?: JoinAction;
};

export type CommunityJoinContext = {
  trackCtx: TrackContext;
  commit: CommitFn;
  track: TrackFn;
  communityId?: string;
  action?: JoinAction;
  result?: CommitResult;
  error?: string;
};

export type CommunityJoinEvent =
  | { type: "SELECT"; communityId: string; action: JoinAction }
  | { type: "START" }
  | { type: "CONFIRM" }
  | { type: "BACK" }
  | { type: "RETRY" }
  | { type: "BROWSE_MORE" };

export const COMMUNITY_JOIN_EVENTS = {
  joinStarted: "cl_community_join_started",
  requestSubmitted: "cl_community_request_submitted",
  joined: "cl_community_joined",
} as const;

export const STATE_TO_SLUG = {
  browsing: "browse",
  detail: "detail",
  joining: "join",
  requesting: "request",
  confirming: "confirm",
  joined: "done",
  error: "error",
} as const;

export type CommunityJoinStateId = keyof typeof STATE_TO_SLUG;
export type CommunityJoinStepSlug = (typeof STATE_TO_SLUG)[CommunityJoinStateId];

export const FIRST_STEP_SLUG: CommunityJoinStepSlug = STATE_TO_SLUG.browsing;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "browsing");

export const SLUG_TO_STATE: Record<CommunityJoinStepSlug, CommunityJoinStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => CommunityJoinStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => CommunityJoinStateId = stepSlugs.toState;

export const communityJoinMachine = setup({
  types: {
    context: {} as CommunityJoinContext,
    events: {} as CommunityJoinEvent,
    input: {} as CommunityJoinInput,
  },
  actors: {
    runCommit: fromPromise<
      CommitResult,
      { communityId: string; action: JoinAction; commit: CommitFn }
    >(({ input, signal }) =>
      input.commit({ communityId: input.communityId, action: input.action, signal }),
    ),
  },
  actions: {
    setSelection: assign({
      communityId: ({ event }) =>
        event.type === "SELECT" ? event.communityId : undefined,
      action: ({ event }) => (event.type === "SELECT" ? event.action : undefined),
    }),
    trackJoinStarted: ({ context }) =>
      context.track(
        COMMUNITY_JOIN_EVENTS.joinStarted,
        { community_id: context.communityId, action: context.action },
        context.trackCtx,
      ),
    trackRequestSubmitted: ({ context }) =>
      context.track(
        COMMUNITY_JOIN_EVENTS.requestSubmitted,
        { community_id: context.communityId },
        context.trackCtx,
      ),
    trackJoined: ({ context }) =>
      context.track(
        COMMUNITY_JOIN_EVENTS.joined,
        {
          community_id: context.communityId,
          action: context.action,
          pending: context.result?.pending ?? false,
          stub: true,
        },
        context.trackCtx,
      ),
  },
  guards: {
    isJoin: ({ context }) => context.action !== "request",
    isRequest: ({ context }) => context.action === "request",
  },
}).createMachine({
  id: "communityJoin",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    commit: input.commit ?? simulateCommit,
    track: input.track ?? defaultTrack,
    communityId: input.communityId,
    action: input.action,
  }),
  initial: "browsing",
  states: {
    browsing: {
      on: {
        SELECT: { target: "detail", actions: "setSelection" },
      },
    },
    detail: {
      on: {
        START: [
          { target: "joining", guard: "isJoin", actions: "trackJoinStarted" },
          { target: "requesting", guard: "isRequest", actions: "trackJoinStarted" },
        ],
        BACK: { target: "browsing" },
      },
    },
    joining: {
      on: {
        CONFIRM: { target: "confirming" },
        BACK: { target: "detail" },
      },
    },
    requesting: {
      entry: "trackRequestSubmitted",
      on: {
        CONFIRM: { target: "confirming" },
        BACK: { target: "detail" },
      },
    },
    confirming: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runCommit",
        src: "runCommit",
        input: ({ context }) => ({
          communityId: context.communityId ?? "",
          action: context.action ?? "join",
          commit: context.commit,
        }),
        onDone: {
          target: "joined",
          actions: [assign({ result: ({ event }) => event.output }), "trackJoined"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "commit failed"),
          }),
        },
      },
    },
    joined: {
      on: {
        BROWSE_MORE: { target: "browsing", actions: assign({ result: undefined }) },
      },
    },
    error: {
      on: {
        RETRY: { target: "confirming" },
        BACK: { target: "detail" },
      },
    },
  },
});

export type CommunityJoinMachine = typeof communityJoinMachine;

export function resolveCommunityJoinSnapshot(args: {
  step: CommunityJoinStateId;
  trackCtx: TrackContext;
  commit?: CommitFn;
  track?: TrackFn;
  communityId?: string;
  action?: JoinAction;
}) {
  const { step, trackCtx, commit, track, communityId, action } = args;
  if (step === "browsing") return undefined;
  const context: CommunityJoinContext = {
    trackCtx,
    commit: commit ?? simulateCommit,
    track: track ?? defaultTrack,
    communityId,
    action: action ?? (step === "requesting" ? "request" : "join"),
  };
  return communityJoinMachine.resolveState({ value: step, context });
}
