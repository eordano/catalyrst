import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  emptyDraft,
  isStepValid,
  simulateCreate as simulateCreateCommunity,
  type CommunityDraft,
  type CreateResult,
} from "@data/lib/catalyst/overlay/community-create";

export type { TrackFn };

export type CreateFn = (args: {
  draft: CommunityDraft;
  signal?: AbortSignal;
}) => Promise<CreateResult>;

export type CommunityInput = {
  trackCtx: TrackContext;
  hasName?: boolean;
  draft?: CommunityDraft;
  create?: CreateFn;
  track?: TrackFn;
};

export type CommunityContext = {
  trackCtx: TrackContext;
  hasName: boolean;
  draft: CommunityDraft;
  create: CreateFn;
  track: TrackFn;
  result?: CreateResult;
  error?: string;
};

export type DraftPatch = Partial<CommunityDraft>;

export type CommunityEvent =
  | { type: "OPEN" }
  | { type: "GET_NAME" }
  | { type: "EDIT"; patch: DraftPatch }
  | { type: "NEXT" }
  | { type: "BACK" }
  | { type: "SUBMIT" }
  | { type: "RETRY" };

export const COMMUNITY_EVENTS = {
  opened: "cl_community_create_opened",
  gateViewed: "cl_community_gate_viewed",
  gatePassed: "cl_community_gate_passed",
  stepCompleted: "cl_community_step_completed",
  reviewReached: "cl_community_review_reached",
  submitAttempted: "cl_community_submit_attempted",
  submitFailed: "cl_community_create_submit_failed",
  created: "cl_community_created",
} as const;

export const STATE_TO_SLUG = {
  create: "create",
  gate: "gate",
  profile: "profile",
  details: "details",
  review: "review",
  submit: "submit",
  done: "done",
} as const;

export type CommunityStateId = keyof typeof STATE_TO_SLUG;
export type CommunityStepSlug = (typeof STATE_TO_SLUG)[CommunityStateId];

export const FIRST_STEP_SLUG: CommunityStepSlug = STATE_TO_SLUG.create;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "create");

export const SLUG_TO_STATE: Record<CommunityStepSlug, CommunityStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => CommunityStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => CommunityStateId = stepSlugs.toState;

export const FORM_ORDER: CommunityStateId[] = ["profile", "details", "review"];

export const simulateCreate: CreateFn = ({ draft, signal }) =>
  simulateCreateCommunity(draft, { signal });

export const communityMachine = setup({
  types: {
    context: {} as CommunityContext,
    events: {} as CommunityEvent,
    input: {} as CommunityInput,
  },
  actors: {
    runCreate: fromPromise<CreateResult, { draft: CommunityDraft; create: CreateFn }>(
      ({ input, signal }) => input.create({ draft: input.draft, signal }),
    ),
  },
  guards: {
    stepValid: ({ context }, params: { step: string }) =>
      isStepValid(params.step, context.draft),
    hasName: ({ context }) => context.hasName,
    lacksName: ({ context }) => !context.hasName,
  },
  actions: {
    applyEdit: assign({
      draft: ({ context, event }) =>
        event.type === "EDIT" ? { ...context.draft, ...event.patch } : context.draft,
    }),
    trackOpened: ({ context }) =>
      context.track(COMMUNITY_EVENTS.opened, {}, context.trackCtx),
    trackGateViewed: ({ context }) =>
      context.track(COMMUNITY_EVENTS.gateViewed, {}, context.trackCtx),
    trackGatePassed: ({ context }) =>
      context.track(
        COMMUNITY_EVENTS.gatePassed,
        { had_name: context.hasName },
        context.trackCtx,
      ),
    trackReviewReached: ({ context }) =>
      context.track(
        COMMUNITY_EVENTS.reviewReached,
        {
          privacy: context.draft.privacy,
          visibility: context.draft.visibility,
          has_thumbnail: context.draft.hasThumbnail,
        },
        context.trackCtx,
      ),
    trackSubmitAttempted: ({ context }) =>
      context.track(
        COMMUNITY_EVENTS.submitAttempted,
        { privacy: context.draft.privacy, visibility: context.draft.visibility },
        context.trackCtx,
      ),
    trackSubmitFailed: ({ context }) =>
      context.track(
        COMMUNITY_EVENTS.submitFailed,
        { error: context.error },
        context.trackCtx,
      ),
    trackCreated: ({ context }) =>
      context.track(
        COMMUNITY_EVENTS.created,
        { community_id: context.result?.id, stub: true },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "communityCreate",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    hasName: input.hasName ?? false,
    draft: input.draft ?? emptyDraft(),
    create: input.create ?? simulateCreate,
    track: input.track ?? defaultTrack,
  }),
  initial: "create",
  on: {
    EDIT: { actions: "applyEdit" },
  },
  states: {
    create: {
      entry: "trackOpened",
      on: {
        OPEN: [
          { target: "gate", guard: "lacksName" },
          { target: "profile", guard: "hasName", actions: "trackGatePassed" },
        ],
      },
    },
    gate: {
      entry: "trackGateViewed",
      on: {
        GET_NAME: { target: "profile", actions: "trackGatePassed" },
        BACK: { target: "create" },
      },
    },
    profile: {
      on: {
        NEXT: { target: "details" },
        BACK: [
          { target: "gate", guard: "lacksName" },
          { target: "create", guard: "hasName" },
        ],
      },
    },
    details: {
      on: {
        NEXT: {
          target: "review",
          guard: { type: "stepValid", params: { step: "details" } },
        },
        BACK: { target: "profile" },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        SUBMIT: {
          target: "submit",
          guard: { type: "stepValid", params: { step: "review" } },
          actions: "trackSubmitAttempted",
        },
        BACK: { target: "details" },
      },
    },
    submit: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runCreate",
        src: "runCreate",
        input: ({ context }) => ({ draft: context.draft, create: context.create }),
        onDone: {
          target: "done",
          actions: [assign({ result: ({ event }) => event.output }), "trackCreated"],
        },
        onError: {
          target: "review",
          actions: [
            assign({
              error: ({ event }) =>
                toErrorMessage(event.error, "create failed"),
            }),
            "trackSubmitFailed",
          ],
        },
      },
    },
    done: {
      type: "final",
    },
  },
});

export type CommunityMachine = typeof communityMachine;

export function emitStepCompleted(
  track: TrackFn,
  ctx: TrackContext,
  from: CommunityStateId,
  to: CommunityStateId,
): void {
  track(COMMUNITY_EVENTS.stepCompleted, { from, to }, ctx);
}

export function resolveCommunitySnapshot(args: {
  step: CommunityStateId;
  trackCtx: TrackContext;
  hasName?: boolean;
  draft?: CommunityDraft;
  create?: CreateFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, hasName, draft, create, track } = args;
  if (step === "create") return undefined;
  const context: CommunityContext = {
    trackCtx,
    hasName: hasName ?? false,
    draft: draft ?? emptyDraft(),
    create: create ?? simulateCreate,
    track: track ?? defaultTrack,
  };
  return communityMachine.resolveState({ value: step, context });
}
