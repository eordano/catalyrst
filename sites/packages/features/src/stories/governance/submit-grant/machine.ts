import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type SubmitResult = {
  proposalId: string;
};

export type SubmitFn = (args: {
  category: string;
  budget: number;
  duration: number;
  signal?: AbortSignal;
}) => Promise<SubmitResult>;

export type { TrackFn };

export type GrantDraft = {
  category?: string;
  tier?: string;
  budget: number;
  duration: number;
};

export type GrantInput = {
  trackCtx: TrackContext;
  submitGrant?: SubmitFn;
  track?: TrackFn;
  draft?: Partial<GrantDraft>;
};

export type GrantContext = {
  trackCtx: TrackContext;
  submitGrant: SubmitFn;
  track: TrackFn;
  draft: GrantDraft;
  result?: SubmitResult;
  error?: string;
};

export type GrantEvent =
  | { type: "PICK_CATEGORY"; category: string }
  | { type: "SET_FUNDING"; budget: number; duration: number; tier?: string }
  | { type: "NEXT" }
  | { type: "BACK" }
  | { type: "SUBMIT" }
  | { type: "RETRY" };

export const GRANT_EVENTS = {
  started: "gv_grant_started",
  fundingSet: "gv_grant_funding_set",
  stepAdvanced: "gv_grant_step_advanced",
  submitAttempted: "gv_grant_submit_attempted",
  submitted: "gv_grant_submitted",
} as const;

export const STATE_TO_SLUG = {
  category: "category",
  funding: "funding",
  general: "general",
  assessment: "assessment",
  review: "review",
  submitting: "submitting",
  submitError: "submit-error",
  success: "success",
} as const;

export type GrantStateId = keyof typeof STATE_TO_SLUG;
export type GrantStepSlug = (typeof STATE_TO_SLUG)[GrantStateId];

export const FIRST_STEP_SLUG: GrantStepSlug = STATE_TO_SLUG.category;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "category");

export const SLUG_TO_STATE: Record<GrantStepSlug, GrantStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => GrantStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => GrantStateId = stepSlugs.toState;

export const simulateSubmit: SubmitFn = async ({ category, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const slug = category.toLowerCase().replace(/[^a-z0-9]+/g, "-");
  return { proposalId: `stub-grant-${slug}-${Date.now().toString(36)}` };
};

const DEFAULT_DRAFT: GrantDraft = { budget: 0, duration: 3 };

export const grantMachine = setup({
  types: {
    context: {} as GrantContext,
    events: {} as GrantEvent,
    input: {} as GrantInput,
  },
  actors: {
    runSubmit: fromPromise<
      SubmitResult,
      { category: string; budget: number; duration: number; submitGrant: SubmitFn }
    >(({ input, signal }) =>
      input.submitGrant({
        category: input.category,
        budget: input.budget,
        duration: input.duration,
        signal,
      }),
    ),
  },
  actions: {
    setCategory: assign({
      draft: ({ context, event }) =>
        event.type === "PICK_CATEGORY"
          ? { ...context.draft, category: event.category }
          : context.draft,
    }),
    setFunding: assign({
      draft: ({ context, event }) =>
        event.type === "SET_FUNDING"
          ? {
              ...context.draft,
              budget: event.budget,
              duration: event.duration,
              tier: event.tier ?? context.draft.tier,
            }
          : context.draft,
    }),
    trackStarted: ({ context }) =>
      context.track(
        GRANT_EVENTS.started,
        { category: context.draft.category },
        context.trackCtx,
      ),
    trackFundingSet: ({ context }) =>
      context.track(
        GRANT_EVENTS.fundingSet,
        {
          category: context.draft.category,
          budget: context.draft.budget,
          duration: context.draft.duration,
          tier: context.draft.tier,
        },
        context.trackCtx,
      ),
    trackStepGeneral: ({ context }) =>
      context.track(
        GRANT_EVENTS.stepAdvanced,
        { to: "general", category: context.draft.category },
        context.trackCtx,
      ),
    trackStepAssessment: ({ context }) =>
      context.track(
        GRANT_EVENTS.stepAdvanced,
        { to: "assessment", category: context.draft.category },
        context.trackCtx,
      ),
    trackStepReview: ({ context }) =>
      context.track(
        GRANT_EVENTS.stepAdvanced,
        { to: "review", category: context.draft.category },
        context.trackCtx,
      ),
    trackSubmitAttempted: ({ context }) =>
      context.track(
        GRANT_EVENTS.submitAttempted,
        {
          category: context.draft.category,
          budget: context.draft.budget,
          duration: context.draft.duration,
          tier: context.draft.tier,
        },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        GRANT_EVENTS.submitted,
        {
          category: context.draft.category,
          budget: context.draft.budget,
          proposal_id: context.result?.proposalId,
          simulated: true,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "grantSubmit",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    submitGrant: input.submitGrant ?? simulateSubmit,
    track: input.track ?? defaultTrack,
    draft: { ...DEFAULT_DRAFT, ...(input.draft ?? {}) },
  }),
  initial: "category",
  states: {
    category: {
      on: {
        PICK_CATEGORY: {
          target: "funding",
          actions: ["setCategory", "trackStarted"],
        },
      },
    },
    funding: {
      on: {
        SET_FUNDING: {
          target: "general",
          actions: ["setFunding", "trackFundingSet"],
        },
        BACK: { target: "category" },
      },
    },
    general: {
      on: {
        NEXT: { target: "assessment", actions: "trackStepAssessment" },
        BACK: { target: "funding" },
      },
    },
    assessment: {
      on: {
        NEXT: { target: "review", actions: "trackStepReview" },
        BACK: { target: "general" },
      },
    },
    review: {
      on: {
        SUBMIT: { target: "submitting", actions: "trackSubmitAttempted" },
        BACK: { target: "assessment" },
      },
    },
    submitting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({
          category: context.draft.category ?? "Platform",
          budget: context.draft.budget,
          duration: context.draft.duration,
          submitGrant: context.submitGrant,
        }),
        onDone: {
          target: "success",
          actions: [
            assign({ result: ({ event }) => event.output }),
            "trackSubmitted",
          ],
        },
        onError: {
          target: "submitError",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "submit failed"),
          }),
        },
      },
    },
    submitError: {
      on: {
        RETRY: { target: "submitting" },
        BACK: { target: "review" },
      },
    },
    success: {
      type: "final",
    },
  },
});

export type GrantMachine = typeof grantMachine;

export function resolveGrantSnapshot(args: {
  step: GrantStateId;
  trackCtx: TrackContext;
  submitGrant?: SubmitFn;
  track?: TrackFn;
  draft?: Partial<GrantDraft>;
}) {
  const { step, trackCtx, submitGrant, track, draft } = args;
  if (step === "category") return undefined;
  const seeded: GrantDraft = {
    category: "Platform",
    tier: "Tier 4",
    budget: 24000,
    duration: 6,
    ...(draft ?? {}),
  };
  const context: GrantContext = {
    trackCtx,
    submitGrant: submitGrant ?? simulateSubmit,
    track: track ?? defaultTrack,
    draft: seeded,
  };
  return grantMachine.resolveState({ value: step, context });
}
