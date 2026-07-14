import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  emptyDraft,
  isStepValid,
  simulateCreateCommunity,
  type CommunityDraft,
  type CreateResult,
} from "@data/lib/catalyst/overlay/create-community";

export type { TrackFn };

export type CreateFn = (args: {
  draft: CommunityDraft;
  signal?: AbortSignal;
}) => Promise<CreateResult>;

export type CommunityInput = {
  trackCtx: TrackContext;
  draft?: CommunityDraft;
  create?: CreateFn;
  track?: TrackFn;
};

export type CommunityContext = {
  trackCtx: TrackContext;
  draft: CommunityDraft;
  create: CreateFn;
  track: TrackFn;
  result?: CreateResult;
  error?: string;
};

export type DraftPatch = Partial<CommunityDraft>;

export type CommunityEvent =
  | { type: "SIGN_IN" }
  | { type: "EDIT"; patch: DraftPatch }
  | { type: "NEXT" }
  | { type: "BACK" }
  | { type: "SUBMIT" }
  | { type: "RETRY" };

export const COMMUNITY_EVENTS = {
  gateViewed: "lp_community_gate_viewed",
  started: "lp_community_started",
  stepCompleted: "lp_community_step_completed",
  reviewReached: "lp_community_review_reached",
  submitAttempted: "lp_community_submit_attempted",
  submitFailed: "lp_community_submit_failed",
  created: "lp_community_created",
} as const;

export const STATE_TO_SLUG = {
  signinGate: "signin-gate",
  basics: "basics",
  thumbnail: "thumbnail",
  privacy: "privacy",
  places: "places",
  review: "review",
  submitting: "submitting",
  created: "created",
} as const;

export type CommunityStateId = keyof typeof STATE_TO_SLUG;
export type CommunityStepSlug = (typeof STATE_TO_SLUG)[CommunityStateId];

export const FIRST_STEP_SLUG: CommunityStepSlug = STATE_TO_SLUG.signinGate;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "signinGate");

export const SLUG_TO_STATE: Record<CommunityStepSlug, CommunityStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => CommunityStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => CommunityStateId = stepSlugs.toState;

export const FORM_ORDER: CommunityStateId[] = [
  "basics",
  "thumbnail",
  "privacy",
  "places",
  "review",
];

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
  },
  actions: {
    applyEdit: assign({
      draft: ({ context, event }) =>
        event.type === "EDIT" ? { ...context.draft, ...event.patch } : context.draft,
    }),
    trackGateViewed: ({ context }) =>
      context.track(COMMUNITY_EVENTS.gateViewed, {}, context.trackCtx),
    trackStarted: ({ context }) =>
      context.track(COMMUNITY_EVENTS.started, {}, context.trackCtx),
    trackReviewReached: ({ context }) =>
      context.track(
        COMMUNITY_EVENTS.reviewReached,
        {
          privacy: context.draft.privacy,
          visibility: context.draft.visibility,
          places: context.draft.placeIds.length,
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
  id: "createCommunity",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    draft: input.draft ?? emptyDraft(),
    create: input.create ?? simulateCreate,
    track: input.track ?? defaultTrack,
  }),
  initial: "signinGate",
  on: {
    EDIT: { actions: "applyEdit" },
  },
  states: {
    signinGate: {
      entry: "trackGateViewed",
      on: {
        SIGN_IN: { target: "basics", actions: "trackStarted" },
      },
    },
    basics: {
      on: {
        NEXT: {
          target: "thumbnail",
          guard: { type: "stepValid", params: { step: "basics" } },
        },
        BACK: { target: "signinGate" },
      },
    },
    thumbnail: {
      on: {
        NEXT: { target: "privacy" },
        BACK: { target: "basics" },
      },
    },
    privacy: {
      on: {
        NEXT: { target: "places" },
        BACK: { target: "thumbnail" },
      },
    },
    places: {
      on: {
        NEXT: { target: "review" },
        BACK: { target: "privacy" },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        SUBMIT: {
          target: "submitting",
          guard: { type: "stepValid", params: { step: "basics" } },
          actions: "trackSubmitAttempted",
        },
        BACK: { target: "places" },
      },
    },
    submitting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runCreate",
        src: "runCreate",
        input: ({ context }) => ({ draft: context.draft, create: context.create }),
        onDone: {
          target: "created",
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
    created: {
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
  draft?: CommunityDraft;
  create?: CreateFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, draft, create, track } = args;
  if (step === "signinGate") return undefined;
  const context: CommunityContext = {
    trackCtx,
    draft: draft ?? emptyDraft(),
    create: create ?? simulateCreate,
    track: track ?? defaultTrack,
  };
  return communityMachine.resolveState({ value: step, context });
}
