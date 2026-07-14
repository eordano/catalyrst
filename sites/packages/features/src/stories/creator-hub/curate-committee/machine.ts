import { delay, makeStepSlugs } from "@core/lib/stories/index";
import { toErrorMessage } from "@core/lib/errors";
import { assign, fromPromise, setup } from "xstate";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import type {
  CommentDecision,
  CommentPostBody,
  StatusFilter,
  TypeFilter,
} from "@data/lib/catalyst/creator-hub/curate-committee";

export type { TrackFn };

export type DecisionStatus = CommentDecision;

export type CurationFilters = {
  status: StatusFilter;
  type: TypeFilter;
  assignee: string;
};

export type AssignBody = { assignee: string };
export type DecideBody = { status: DecisionStatus };

export type AssignResult = { id: string; assignee: string; simulated: true };
export type DecideResult = {
  id: string;
  status: DecisionStatus;
  updated: number;
  comment?: { postId: number; link: string; raw: string };
  simulated: true;
};

export type AssignFn = (args: {
  id: string;
  body: AssignBody;
  signal?: AbortSignal;
}) => Promise<AssignResult>;

export type DecideFn = (args: {
  id: string;
  body: DecideBody;
  comment?: CommentPostBody;
  signal?: AbortSignal;
}) => Promise<DecideResult>;

export type CurationInput = {
  trackCtx: TrackContext;
  count: number;
  youAddress: string;
  filters?: CurationFilters;
  assign?: AssignFn;
  decide?: DecideFn;
  track?: TrackFn;
};

export type CurationContext = {
  trackCtx: TrackContext;
  count: number;
  youAddress: string;
  filters: CurationFilters;
  activeId?: string;
  activeTopicId?: number | null;
  decision?: DecisionStatus;
  comment: string;
  assignResult?: AssignResult;
  decideResult?: DecideResult;
  assign: AssignFn;
  decide: DecideFn;
  track: TrackFn;
  error?: string;
};

export type CurationEvent =
  | { type: "FILTER"; filters: CurationFilters }
  | { type: "ASSIGN"; id: string }
  | { type: "OPEN_REVIEW"; id: string; topicId?: number | null }
  | { type: "DRAFT_DECISION"; status: DecisionStatus }
  | { type: "EDIT_COMMENT"; comment: string }
  | { type: "SUBMIT"; comment: string }
  | { type: "BACK" };

export const CURATION_EVENTS = {
  viewed: "bd_curation_viewed",
  filtered: "bd_curation_filtered",
  assigned: "bd_curation_assigned",
  reviewOpened: "bd_curation_review_opened",
  commentAdded: "bd_curation_comment_added",
  decided: "bd_curation_decided",
} as const;

const DEFAULT_FILTERS: CurationFilters = {
  status: "ALL_STATUS",
  type: "ALL_TYPES",
  assignee: "all",
};

export const STATE_TO_SLUG = {
  dashboard: "dashboard",
  assigning: "assign",
  reviewing: "review",
  commenting: "comment",
  deciding: "decide",
  decided: "decided",
} as const;

export type CurationStateId = keyof typeof STATE_TO_SLUG;
export type CurationStepSlug = (typeof STATE_TO_SLUG)[CurationStateId];

export const FIRST_STEP_SLUG: CurationStepSlug = STATE_TO_SLUG.dashboard;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "dashboard");

export const SLUG_TO_STATE: Record<CurationStepSlug, CurationStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => CurationStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => CurationStateId = stepSlugs.toState;

export const simulateAssign: AssignFn = async ({ id, body, signal }) => {
  await delay(300, signal);
  return { id, assignee: body.assignee, simulated: true };
};

export const simulateDecide: DecideFn = async ({ id, body, comment, signal }) => {
  await delay(400, signal);
  if (body.status !== "approved" && body.status !== "rejected") {
    throw new Error("Invalid Status provided");
  }
  const result: DecideResult = { id, status: body.status, updated: 1, simulated: true };
  if (comment && comment.raw.trim()) {
    const topic = comment.topic_id ?? 0;
    result.comment = {
      postId: Math.floor(Math.random() * 1e6),
      link: `https://forum.decentraland.org/t/${topic}`,
      raw: comment.raw.trim(),
    };
  }
  return result;
};

export const curationMachine = setup({
  types: {
    context: {} as CurationContext,
    events: {} as CurationEvent,
    input: {} as CurationInput,
  },
  actors: {
    runAssign: fromPromise<AssignResult, { id: string; body: AssignBody; assign: AssignFn }>(
      ({ input, signal }) => input.assign({ id: input.id, body: input.body, signal }),
    ),
    runDecide: fromPromise<
      DecideResult,
      { id: string; body: DecideBody; comment?: CommentPostBody; decide: DecideFn }
    >(({ input, signal }) =>
      input.decide({ id: input.id, body: input.body, comment: input.comment, signal }),
    ),
  },
  actions: {
    setFilters: assign({
      filters: ({ context, event }) =>
        event.type === "FILTER" ? event.filters : context.filters,
    }),
    trackFiltered: ({ context, event }) => {
      if (event.type !== "FILTER") return;
      context.track(
        CURATION_EVENTS.filtered,
        {
          status: event.filters.status,
          type: event.filters.type,
          assignee: event.filters.assignee,
        },
        context.trackCtx,
      );
    },
    setActiveFromAssign: assign({
      activeId: ({ context, event }) =>
        event.type === "ASSIGN" ? event.id : context.activeId,
    }),
    trackAssigned: ({ context, event }) => {
      if (event.type !== "ASSIGN") return;
      context.track(CURATION_EVENTS.assigned, { id: event.id }, context.trackCtx);
    },
    setActiveFromReview: assign({
      activeId: ({ context, event }) =>
        event.type === "OPEN_REVIEW" ? event.id : context.activeId,
      activeTopicId: ({ context, event }) =>
        event.type === "OPEN_REVIEW" ? (event.topicId ?? null) : context.activeTopicId,
    }),
    trackReviewOpened: ({ context, event }) => {
      if (event.type !== "OPEN_REVIEW") return;
      context.track(CURATION_EVENTS.reviewOpened, { id: event.id }, context.trackCtx);
    },
    setDraftDecision: assign({
      decision: ({ context, event }) =>
        event.type === "DRAFT_DECISION" ? event.status : context.decision,
    }),
    editComment: assign({
      comment: ({ context, event }) =>
        event.type === "EDIT_COMMENT" ? event.comment : context.comment,
    }),
    setCommentFromSubmit: assign({
      comment: ({ context, event }) =>
        event.type === "SUBMIT" ? event.comment : context.comment,
    }),
    trackCommentAdded: ({ context }) => {
      const raw = (context.comment ?? "").trim();
      if (!raw) return;
      context.track(
        CURATION_EVENTS.commentAdded,
        {
          id: context.activeId,
          decision: context.decision,
          has_comment: true,
          length: raw.length,
          topic_id: context.activeTopicId ?? null,
          stub: true,
        },
        context.trackCtx,
      );
    },
    trackDecided: ({ context }) =>
      context.track(
        CURATION_EVENTS.decided,
        {
          id: context.activeId,
          status: context.decision,
          updated: context.decideResult?.updated,
          has_comment: !!(context.comment ?? "").trim(),
          stub: true,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "curateCommittee",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    count: input.count,
    youAddress: input.youAddress,
    filters: input.filters ?? DEFAULT_FILTERS,
    comment: "",
    assign: input.assign ?? simulateAssign,
    decide: input.decide ?? simulateDecide,
    track: input.track ?? defaultTrack,
  }),
  initial: "dashboard",
  states: {
    dashboard: {
      on: {
        FILTER: { actions: ["setFilters", "trackFiltered"] },
        ASSIGN: { target: "assigning", actions: ["setActiveFromAssign", "trackAssigned"] },
        OPEN_REVIEW: {
          target: "reviewing",
          actions: ["setActiveFromReview", "trackReviewOpened"],
        },
      },
    },
    assigning: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runAssign",
        src: "runAssign",
        input: ({ context }) => ({
          id: context.activeId ?? "",
          body: { assignee: context.youAddress },
          assign: context.assign,
        }),
        onDone: {
          target: "dashboard",
          actions: assign({ assignResult: ({ event }) => event.output }),
        },
        onError: {
          target: "dashboard",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "assign failed"),
          }),
        },
      },
    },
    reviewing: {
      on: {
        DRAFT_DECISION: { target: "commenting", actions: "setDraftDecision" },
        BACK: { target: "dashboard" },
      },
    },
    commenting: {
      on: {
        EDIT_COMMENT: { actions: "editComment" },
        SUBMIT: { target: "deciding", actions: "setCommentFromSubmit" },
        BACK: { target: "reviewing" },
      },
    },
    deciding: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runDecide",
        src: "runDecide",
        input: ({ context }) => ({
          id: context.activeId ?? "",
          body: { status: context.decision ?? "approved" },
          comment: (context.comment ?? "").trim()
            ? { raw: context.comment.trim(), topic_id: context.activeTopicId ?? null }
            : undefined,
          decide: context.decide,
        }),
        onDone: {
          target: "decided",
          actions: [
            assign({ decideResult: ({ event }) => event.output }),
            "trackCommentAdded",
            "trackDecided",
          ],
        },
        onError: {
          target: "commenting",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "decision failed"),
          }),
        },
      },
    },
    decided: {
      type: "final",
    },
  },
});

export type CurationMachine = typeof curationMachine;

export function resolveCurationSnapshot(args: {
  step: CurationStateId;
  trackCtx: TrackContext;
  count: number;
  youAddress: string;
  filters?: CurationFilters;
  activeId?: string;
  activeTopicId?: number | null;
  decision?: DecisionStatus;
  comment?: string;
  assign?: AssignFn;
  decide?: DecideFn;
  track?: TrackFn;
}) {
  const {
    step,
    trackCtx,
    count,
    youAddress,
    filters,
    activeId,
    activeTopicId = null,
    decision = "approved",
    comment = "",
    assign: assignFn,
    decide: decideFn,
    track,
  } = args;
  if (step === "dashboard") return undefined;
  const decisionSteps = step === "commenting" || step === "deciding" || step === "decided";
  const context: CurationContext = {
    trackCtx,
    count,
    youAddress,
    filters: filters ?? DEFAULT_FILTERS,
    activeId,
    activeTopicId,
    decision: decisionSteps ? decision : undefined,
    comment: decisionSteps ? comment : "",
    assign: assignFn ?? simulateAssign,
    decide: decideFn ?? simulateDecide,
    track: track ?? defaultTrack,
  };
  return curationMachine.resolveState({ value: step, context });
}
