import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type SubmitResult = {
  proposalId: string;
  published: boolean;
};

export type SubmitFn = (args: {
  tenderId: string;
  budget: number;
  duration: number;
  signal?: AbortSignal;
}) => Promise<SubmitResult>;

export type { TrackFn };

export type BidDraft = {
  tenderId: string;
  budget: number;
  duration: number;
};

export type BidInput = {
  trackCtx: TrackContext;
  tenderId: string;
  submitBid?: SubmitFn;
  track?: TrackFn;
  draft?: Partial<BidDraft>;
};

export type BidContext = {
  trackCtx: TrackContext;
  submitBid: SubmitFn;
  track: TrackFn;
  draft: BidDraft;
  result?: SubmitResult;
  error?: string;
};

export type BidEvent =
  | { type: "CONTINUE" }
  | { type: "SET_FUNDING"; budget: number; duration: number }
  | { type: "NEXT" }
  | { type: "BACK" }
  | { type: "SUBMIT" }
  | { type: "RETRY" };

export const BID_EVENTS = {
  started: "gv_bid_started",
  fundingSet: "gv_bid_funding_set",
  stepAdvanced: "gv_bid_step_advanced",
  submitAttempted: "gv_bid_submit_attempted",
  submitted: "gv_bid_submitted",
} as const;

export const STATE_TO_SLUG = {
  parents: "parents",
  funding: "funding",
  general: "general",
  review: "review",
  submitting: "submitting",
  submitError: "submit-error",
  success: "success",
} as const;

export type BidStateId = keyof typeof STATE_TO_SLUG;
export type BidStepSlug = (typeof STATE_TO_SLUG)[BidStateId];

export const FIRST_STEP_SLUG: BidStepSlug = STATE_TO_SLUG.parents;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "parents");

export const SLUG_TO_STATE: Record<BidStepSlug, BidStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => BidStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => BidStateId = stepSlugs.toState;

export const failClosedSubmit: SubmitFn = async () => {
  throw new Error("bid submission unavailable: DAO governance signer not configured");
};

const DEFAULT_DRAFT: BidDraft = { tenderId: "", budget: 0, duration: 1 };

export const bidMachine = setup({
  types: {
    context: {} as BidContext,
    events: {} as BidEvent,
    input: {} as BidInput,
  },
  actors: {
    runSubmit: fromPromise<
      SubmitResult,
      { tenderId: string; budget: number; duration: number; submitBid: SubmitFn }
    >(({ input, signal }) =>
      input.submitBid({
        tenderId: input.tenderId,
        budget: input.budget,
        duration: input.duration,
        signal,
      }),
    ),
  },
  actions: {
    setFunding: assign({
      draft: ({ context, event }) =>
        event.type === "SET_FUNDING"
          ? { ...context.draft, budget: event.budget, duration: event.duration }
          : context.draft,
    }),
    trackStarted: ({ context }) =>
      context.track(
        BID_EVENTS.started,
        { tender_id: context.draft.tenderId },
        context.trackCtx,
      ),
    trackFundingSet: ({ context }) =>
      context.track(
        BID_EVENTS.fundingSet,
        {
          tender_id: context.draft.tenderId,
          budget: context.draft.budget,
          duration: context.draft.duration,
        },
        context.trackCtx,
      ),
    trackStepReview: ({ context }) =>
      context.track(
        BID_EVENTS.stepAdvanced,
        { to: "review", tender_id: context.draft.tenderId },
        context.trackCtx,
      ),
    trackSubmitAttempted: ({ context }) =>
      context.track(
        BID_EVENTS.submitAttempted,
        {
          tender_id: context.draft.tenderId,
          budget: context.draft.budget,
          duration: context.draft.duration,
        },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        BID_EVENTS.submitted,
        {
          tender_id: context.draft.tenderId,
          budget: context.draft.budget,
          proposal_id: context.result?.proposalId,
          published: context.result?.published ?? false,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "bidSubmit",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    submitBid: input.submitBid ?? failClosedSubmit,
    track: input.track ?? defaultTrack,
    draft: { ...DEFAULT_DRAFT, tenderId: input.tenderId, ...(input.draft ?? {}) },
  }),
  initial: "parents",
  states: {
    parents: {
      on: {
        CONTINUE: { target: "funding", actions: "trackStarted" },
      },
    },
    funding: {
      on: {
        SET_FUNDING: { target: "general", actions: ["setFunding", "trackFundingSet"] },
        BACK: { target: "parents" },
      },
    },
    general: {
      on: {
        NEXT: { target: "review", actions: "trackStepReview" },
        BACK: { target: "funding" },
      },
    },
    review: {
      on: {
        SUBMIT: { target: "submitting", actions: "trackSubmitAttempted" },
        BACK: { target: "general" },
      },
    },
    submitting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({
          tenderId: context.draft.tenderId,
          budget: context.draft.budget,
          duration: context.draft.duration,
          submitBid: context.submitBid,
        }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackSubmitted"],
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

export type BidMachine = typeof bidMachine;

export function resolveBidSnapshot(args: {
  step: BidStateId;
  trackCtx: TrackContext;
  tenderId: string;
  submitBid?: SubmitFn;
  track?: TrackFn;
  draft?: Partial<BidDraft>;
}) {
  const { step, trackCtx, tenderId, submitBid, track, draft } = args;
  if (step === "parents") return undefined;
  const seeded: BidDraft = {
    tenderId,
    budget: 90000,
    duration: 4,
    ...(draft ?? {}),
  };
  const context: BidContext = {
    trackCtx,
    submitBid: submitBid ?? failClosedSubmit,
    track: track ?? defaultTrack,
    draft: seeded,
  };
  return bidMachine.resolveState({ value: step, context });
}
