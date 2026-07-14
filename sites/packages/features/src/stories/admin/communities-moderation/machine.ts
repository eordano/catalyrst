import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  requestSuspension,
  type CommunityDecision,
  type CommunityStatus,
  type SuspendResult,
} from "@data/lib/catalyst/admin/community-moderation";

export type { TrackFn };

export type SuspendFn = (args: {
  communityId: string;
  decision: CommunityDecision;
  reason?: string;
  signal?: AbortSignal;
}) => Promise<SuspendResult>;

export type ModerateInput = {
  trackCtx: TrackContext;
  suspend?: SuspendFn;
  track?: TrackFn;
  total?: number;
};

export type ModerateContext = {
  trackCtx: TrackContext;
  suspend: SuspendFn;
  track: TrackFn;
  total: number;
  statusFilter: CommunityStatus;
  communityId?: string;
  decision?: CommunityDecision;
  reason?: string;
  result?: SuspendResult;
  error?: string;
};

export type ModerateEvent =
  | { type: "SIGN_IN" }
  | { type: "SET_FILTER"; status: CommunityStatus; total?: number }
  | { type: "OPEN"; communityId: string }
  | { type: "CLOSE" }
  | { type: "DECIDE"; decision: CommunityDecision; reason?: string }
  | { type: "CANCEL" }
  | { type: "CONFIRM" }
  | { type: "RETRY" }
  | { type: "CONTINUE" };

export const MODERATE_EVENTS = {
  gateViewed: "admin_community_gate_viewed",
  authenticated: "admin_community_authenticated",
  listViewed: "admin_community_list_viewed",
  reviewed: "admin_community_reviewed",
  decisionSelected: "admin_community_decision_selected",
  committed: "admin_community_suspension_committed",
  failed: "admin_community_moderation_failed",
} as const;

export const STATE_TO_SLUG = {
  authGate: "auth-gate",
  list: "list",
  reviewCommunity: "review-community",
  decision: "decision",
  submitting: "submitting",
  moderated: "moderated",
} as const;

export type ModerateStateId = keyof typeof STATE_TO_SLUG;
export type ModerateStepSlug = (typeof STATE_TO_SLUG)[ModerateStateId];

export const FIRST_STEP_SLUG: ModerateStepSlug = STATE_TO_SLUG.authGate;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "authGate");

export const SLUG_TO_STATE: Record<ModerateStepSlug, ModerateStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => ModerateStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => ModerateStateId = stepSlugs.toState;

export const defaultSuspend: SuspendFn = ({ communityId, decision, reason, signal }) =>
  requestSuspension({ communityId, decision, reason }, { signal });

export const moderateMachine = setup({
  types: {
    context: {} as ModerateContext,
    events: {} as ModerateEvent,
    input: {} as ModerateInput,
  },
  actors: {
    runSuspend: fromPromise<
      SuspendResult,
      { communityId: string; decision: CommunityDecision; reason?: string; suspend: SuspendFn }
    >(({ input, signal }) =>
      input.suspend({
        communityId: input.communityId,
        decision: input.decision,
        reason: input.reason,
        signal,
      }),
    ),
  },
  actions: {
    trackGateViewed: ({ context }) =>
      context.track(MODERATE_EVENTS.gateViewed, {}, context.trackCtx),
    trackAuthenticated: ({ context }) =>
      context.track(MODERATE_EVENTS.authenticated, { simulated_moderator: true }, context.trackCtx),
    trackListViewed: ({ context }) =>
      context.track(
        MODERATE_EVENTS.listViewed,
        { total: context.total, status_filter: context.statusFilter },
        context.trackCtx,
      ),
    setFilter: assign({
      statusFilter: ({ context, event }) =>
        event.type === "SET_FILTER" ? event.status : context.statusFilter,
      total: ({ context, event }) =>
        event.type === "SET_FILTER" && event.total !== undefined ? event.total : context.total,
    }),
    trackFilterViewed: ({ context, event }) => {
      if (event.type !== "SET_FILTER") return;
      context.track(
        MODERATE_EVENTS.listViewed,
        { total: context.total, status_filter: context.statusFilter },
        context.trackCtx,
      );
    },
    setCommunity: assign({
      communityId: ({ event }) => (event.type === "OPEN" ? event.communityId : undefined),
      decision: undefined,
      reason: undefined,
      result: undefined,
      error: undefined,
    }),
    trackReviewed: ({ context, event }) => {
      if (event.type !== "OPEN") return;
      context.track(MODERATE_EVENTS.reviewed, { community_id: event.communityId }, context.trackCtx);
    },
    setDecision: assign({
      decision: ({ event }) => (event.type === "DECIDE" ? event.decision : undefined),
      reason: ({ event }) => (event.type === "DECIDE" ? event.reason : undefined),
    }),
    trackDecisionSelected: ({ context, event }) => {
      if (event.type !== "DECIDE") return;
      context.track(
        MODERATE_EVENTS.decisionSelected,
        { community_id: context.communityId, decision: event.decision },
        context.trackCtx,
      );
    },
    trackCommitted: ({ context }) =>
      context.track(
        MODERATE_EVENTS.committed,
        {
          community_id: context.communityId,
          suspended: context.result?.suspended ?? context.decision === "suspend",
          has_reason: Boolean(context.reason && context.reason.trim().length > 0),
        },
        context.trackCtx,
      ),
    trackFailed: ({ context }) =>
      context.track(
        MODERATE_EVENTS.failed,
        { community_id: context.communityId, error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "adminCommunitiesModerate",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    suspend: input.suspend ?? defaultSuspend,
    track: input.track ?? defaultTrack,
    total: input.total ?? 0,
    statusFilter: "all",
  }),
  initial: "authGate",
  states: {
    authGate: {
      entry: "trackGateViewed",
      on: {
        SIGN_IN: { target: "list", actions: "trackAuthenticated" },
      },
    },
    list: {
      entry: "trackListViewed",
      on: {
        SET_FILTER: { actions: ["setFilter", "trackFilterViewed"] },
        OPEN: { target: "reviewCommunity", actions: ["setCommunity", "trackReviewed"] },
      },
    },
    reviewCommunity: {
      on: {
        DECIDE: { target: "decision", actions: ["setDecision", "trackDecisionSelected"] },
        CLOSE: { target: "list" },
      },
    },
    decision: {
      on: {
        CONFIRM: {
          target: "submitting",
          actions: [assign({ error: undefined })],
        },
        DECIDE: { actions: ["setDecision", "trackDecisionSelected"] },
        CANCEL: { target: "reviewCommunity", actions: assign({ error: undefined }) },
      },
    },
    submitting: {
      invoke: {
        id: "runSuspend",
        src: "runSuspend",
        input: ({ context }) => ({
          communityId: context.communityId ?? "",
          decision: context.decision ?? "suspend",
          reason: context.reason,
          suspend: context.suspend,
        }),
        onDone: {
          target: "moderated",
          actions: [assign({ result: ({ event }) => event.output }), "trackCommitted"],
        },
        onError: {
          target: "decision",
          actions: [
            assign({
              error: ({ event }) =>
                toErrorMessage(event.error, "moderation failed"),
            }),
            "trackFailed",
          ],
        },
      },
    },
    moderated: {
      on: {
        CONTINUE: {
          target: "list",
          actions: assign({
            communityId: undefined,
            decision: undefined,
            reason: undefined,
            result: undefined,
            error: undefined,
          }),
        },
      },
    },
  },
});

export type ModerateMachine = typeof moderateMachine;

export function resolveModerateSnapshot(args: {
  step: ModerateStateId;
  trackCtx: TrackContext;
  suspend?: SuspendFn;
  track?: TrackFn;
  total?: number;
  statusFilter?: CommunityStatus;
  communityId?: string;
  decision?: CommunityDecision;
}) {
  const { step, trackCtx, suspend, track, total, statusFilter, communityId, decision } = args;
  if (step === "authGate") return undefined;
  const context: ModerateContext = {
    trackCtx,
    suspend: suspend ?? defaultSuspend,
    track: track ?? defaultTrack,
    total: total ?? 0,
    statusFilter: statusFilter ?? "all",
    communityId: step === "list" ? undefined : communityId,
    decision:
      step === "decision" || step === "submitting" || step === "moderated"
        ? (decision ?? "suspend")
        : undefined,
  };
  return moderateMachine.resolveState({ value: step, context });
}
