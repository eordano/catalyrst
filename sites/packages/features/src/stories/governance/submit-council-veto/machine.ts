import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import {
  failClosedCreateProposal,
  buildProposalPayload,
  type CreatedProposal,
  type CreateProposalFn,
} from "@data/lib/catalyst/governance/submit-council-veto";
import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type Details = { decisionUrl: string };
export type Reasons = { reasons: string; suggestions: string };

export type CreateFn = (args: {
  details: Details;
  reasons: Reasons;
  coAuthors: string[];
  signal?: AbortSignal;
}) => Promise<CreatedProposal>;

export type SubmitCouncilVetoInput = {
  trackCtx: TrackContext;
  create?: CreateFn;
  track?: TrackFn;
};

export type SubmitCouncilVetoContext = {
  trackCtx: TrackContext;
  create: CreateFn;
  track: TrackFn;
  details: Details;
  reasons: Reasons;
  coAuthors: string[];
  result?: CreatedProposal;
  error?: string;
};

export type SubmitCouncilVetoEvent =
  | { type: "FILL_DETAILS"; decisionUrl: string }
  | { type: "URL_INVALID" }
  | { type: "FILL_REASONS"; reasons: string; suggestions?: string }
  | { type: "FILL_COAUTHORS"; coAuthors?: string[] }
  | { type: "SUBMIT" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const COUNCIL_VETO_EVENTS = {
  started: "gv_council_veto_started",
  urlInvalid: "gv_council_veto_url_invalid",
  reasonsFilled: "gv_council_veto_reasons_filled",
  coauthorsSet: "gv_council_veto_coauthors_set",
  reviewReached: "gv_council_veto_review_reached",
  submitting: "gv_council_veto_submitting",
  submitted: "gv_council_veto_submitted",
  submitError: "gv_council_veto_submit_error",
} as const;

export const STATE_TO_SLUG = {
  details: "details",
  reasons: "reasons",
  coauthors: "coauthors",
  review: "review",
  submitting: "submitting",
  success: "success",
  error: "error",
} as const;

export type CouncilVetoStateId = keyof typeof STATE_TO_SLUG;
export type CouncilVetoStepSlug = (typeof STATE_TO_SLUG)[CouncilVetoStateId];

export const FIRST_STEP_SLUG: CouncilVetoStepSlug = STATE_TO_SLUG.details;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "details");

export const SLUG_TO_STATE: Record<CouncilVetoStepSlug, CouncilVetoStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => CouncilVetoStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => CouncilVetoStateId = stepSlugs.toState;

const EMPTY_DETAILS: Details = { decisionUrl: "" };
const EMPTY_REASONS: Reasons = { reasons: "", suggestions: "" };

export function makeCreate(create: CreateProposalFn): CreateFn {
  return ({ details, reasons, coAuthors, signal }) =>
    create({
      payload: buildProposalPayload({
        decisionUrl: details.decisionUrl,
        reasons: reasons.reasons,
        suggestions: reasons.suggestions,
        coAuthors,
      }),
      signal,
    });
}

export const defaultCreate: CreateFn = makeCreate(failClosedCreateProposal);

export const submitCouncilVetoMachine = setup({
  types: {
    context: {} as SubmitCouncilVetoContext,
    events: {} as SubmitCouncilVetoEvent,
    input: {} as SubmitCouncilVetoInput,
  },
  actors: {
    runCreate: fromPromise<
      CreatedProposal,
      { details: Details; reasons: Reasons; coAuthors: string[]; create: CreateFn }
    >(({ input, signal }) =>
      input.create({
        details: input.details,
        reasons: input.reasons,
        coAuthors: input.coAuthors,
        signal,
      }),
    ),
  },
  actions: {
    setDetails: assign({
      details: ({ event }) =>
        event.type === "FILL_DETAILS"
          ? { decisionUrl: event.decisionUrl }
          : EMPTY_DETAILS,
    }),
    setReasons: assign({
      reasons: ({ event }) =>
        event.type === "FILL_REASONS"
          ? { reasons: event.reasons, suggestions: event.suggestions ?? "" }
          : EMPTY_REASONS,
    }),
    setCoAuthors: assign({
      coAuthors: ({ event }) =>
        event.type === "FILL_COAUTHORS" ? event.coAuthors ?? [] : [],
    }),
    trackStarted: ({ context }) =>
      context.track(COUNCIL_VETO_EVENTS.started, {}, context.trackCtx),
    trackUrlInvalid: ({ context }) =>
      context.track(COUNCIL_VETO_EVENTS.urlInvalid, {}, context.trackCtx),
    trackReasonsFilled: ({ context }) =>
      context.track(
        COUNCIL_VETO_EVENTS.reasonsFilled,
        {
          reasons_length: context.reasons.reasons.length,
          has_suggestions: context.reasons.suggestions.trim().length > 0,
        },
        context.trackCtx,
      ),
    trackCoauthorsSet: ({ context }) =>
      context.track(
        COUNCIL_VETO_EVENTS.coauthorsSet,
        { coauthors: context.coAuthors.length },
        context.trackCtx,
      ),
    trackReviewReached: ({ context }) =>
      context.track(COUNCIL_VETO_EVENTS.reviewReached, {}, context.trackCtx),
    trackSubmitting: ({ context }) =>
      context.track(COUNCIL_VETO_EVENTS.submitting, {}, context.trackCtx),
    trackSubmitted: ({ context }) =>
      context.track(
        COUNCIL_VETO_EVENTS.submitted,
        { proposal_id: context.result?.id },
        context.trackCtx,
      ),
    trackSubmitError: ({ context }) =>
      context.track(
        COUNCIL_VETO_EVENTS.submitError,
        { error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "submitCouncilVeto",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    create: input.create ?? defaultCreate,
    track: input.track ?? defaultTrack,
    details: EMPTY_DETAILS,
    reasons: EMPTY_REASONS,
    coAuthors: [],
  }),
  initial: "details",
  states: {
    details: {
      on: {
        FILL_DETAILS: {
          target: "reasons",
          actions: ["setDetails", "trackStarted"],
        },
        URL_INVALID: { actions: "trackUrlInvalid" },
      },
    },
    reasons: {
      on: {
        FILL_REASONS: {
          target: "coauthors",
          actions: ["setReasons", "trackReasonsFilled"],
        },
        BACK: { target: "details" },
      },
    },
    coauthors: {
      on: {
        FILL_COAUTHORS: {
          target: "review",
          actions: ["setCoAuthors", "trackCoauthorsSet"],
        },
        BACK: { target: "reasons" },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        SUBMIT: { target: "submitting" },
        BACK: { target: "coauthors" },
      },
    },
    submitting: {
      entry: [assign({ error: undefined }), "trackSubmitting"],
      invoke: {
        id: "runCreate",
        src: "runCreate",
        input: ({ context }) => ({
          details: context.details,
          reasons: context.reasons,
          coAuthors: context.coAuthors,
          create: context.create,
        }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackSubmitted"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "createProposal failed"),
          }),
        },
      },
    },
    success: {
      type: "final",
    },
    error: {
      entry: "trackSubmitError",
      on: {
        RETRY: { target: "submitting" },
        BACK: { target: "review" },
      },
    },
  },
});

export type SubmitCouncilVetoMachine = typeof submitCouncilVetoMachine;

export function resolveCouncilVetoSnapshot(args: {
  step: CouncilVetoStateId;
  trackCtx: TrackContext;
  create?: CreateFn;
  track?: TrackFn;
  details?: Details;
  reasons?: Reasons;
  coAuthors?: string[];
}) {
  const { step, trackCtx, create, track, details, reasons, coAuthors } = args;
  if (step === "details") return undefined;
  const context: SubmitCouncilVetoContext = {
    trackCtx,
    create: create ?? defaultCreate,
    track: track ?? defaultTrack,
    details: details ?? { ...EMPTY_DETAILS },
    reasons: reasons ?? { ...EMPTY_REASONS },
    coAuthors: coAuthors ?? [],
  };
  return submitCouncilVetoMachine.resolveState({ value: step, context });
}
