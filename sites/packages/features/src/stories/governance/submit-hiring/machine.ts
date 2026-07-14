import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  validateTarget,
  validateReasons,
  buildProposalPayload,
  failClosedCreateProposal,
  type FieldErrors,
  type HiringRequest,
  type CreatedProposal,
  type CreateProposalFn,
  type SubmitHiringData,
} from "@data/lib/catalyst/governance/submit-hiring";

export type { TrackFn };

export type HiringDraft = {
  committee: string;
  address: string;
  reasons: string;
  evidence: string;
  coAuthors: string[];
};

export type ErrorCopy = SubmitHiringData["copy"]["errors"];

export type SubmitFn = (args: {
  request: HiringRequest;
  draft: HiringDraft;
  signal?: AbortSignal;
}) => Promise<CreatedProposal>;

export type HiringInput = {
  trackCtx: TrackContext;
  request: HiringRequest;
  errorCopy: ErrorCopy;
  submit?: SubmitFn;
  track?: TrackFn;
};

export type HiringContext = {
  trackCtx: TrackContext;
  request: HiringRequest;
  errorCopy: ErrorCopy;
  submit: SubmitFn;
  track: TrackFn;
  draft: HiringDraft;
  errors: FieldErrors;
  result?: CreatedProposal;
  error?: string;
};

export type HiringEvent =
  | { type: "SUBMIT_TARGET"; committee: string; address: string }
  | {
      type: "SUBMIT_REASONS";
      reasons: string;
      evidence: string;
      coAuthors: string[];
    }
  | { type: "CONFIRM" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const HIRING_EVENTS = {
  started: "gv_hiring_started",
  targetSubmitted: "gv_hiring_target_submitted",
  targetInvalid: "gv_hiring_target_invalid",
  reasonsSubmitted: "gv_hiring_reasons_submitted",
  reviewReached: "gv_hiring_review_reached",
  submitting: "gv_hiring_submitting",
  submitted: "gv_hiring_submitted",
  submitError: "gv_hiring_submit_error",
} as const;

export const STATE_TO_SLUG = {
  target: "target",
  reasons: "reasons",
  review: "review",
  submitting: "submitting",
  success: "success",
  error: "error",
} as const;

export type HiringStateId = keyof typeof STATE_TO_SLUG;
export type HiringStepSlug = (typeof STATE_TO_SLUG)[HiringStateId];

export const FIRST_STEP_SLUG: HiringStepSlug = STATE_TO_SLUG.target;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "target");

export const SLUG_TO_STATE: Record<HiringStepSlug, HiringStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => HiringStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => HiringStateId = stepSlugs.toState;

export function emptyDraft(): HiringDraft {
  return { committee: "", address: "", reasons: "", evidence: "", coAuthors: [] };
}

export function makeSubmit(create: CreateProposalFn): SubmitFn {
  return ({ request, draft, signal }) =>
    create({ payload: buildProposalPayload(request, draft), signal });
}

export const defaultSubmit: SubmitFn = makeSubmit(failClosedCreateProposal);

export const hiringMachine = setup({
  types: {
    context: {} as HiringContext,
    events: {} as HiringEvent,
    input: {} as HiringInput,
  },
  actors: {
    runSubmit: fromPromise<
      CreatedProposal,
      { request: HiringRequest; draft: HiringDraft; submit: SubmitFn }
    >(({ input, signal }) =>
      input.submit({ request: input.request, draft: input.draft, signal }),
    ),
  },
  guards: {
    targetValid: ({ context, event }) => {
      if (event.type !== "SUBMIT_TARGET") return false;
      return (
        Object.keys(
          validateTarget({
            request: context.request,
            committee: event.committee,
            address: event.address,
            errorCopy: context.errorCopy,
          }),
        ).length === 0
      );
    },
    reasonsValid: ({ context, event }) => {
      if (event.type !== "SUBMIT_REASONS") return false;
      return (
        Object.keys(
          validateReasons({
            reasons: event.reasons,
            evidence: event.evidence,
            errorCopy: context.errorCopy,
          }),
        ).length === 0
      );
    },
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(HIRING_EVENTS.started, { request: context.request }, context.trackCtx),
    saveTarget: assign({
      draft: ({ context, event }) =>
        event.type === "SUBMIT_TARGET"
          ? { ...context.draft, committee: event.committee, address: event.address }
          : context.draft,
      errors: () => ({}),
    }),
    setTargetErrors: assign({
      errors: ({ context, event }) =>
        event.type === "SUBMIT_TARGET"
          ? validateTarget({
              request: context.request,
              committee: event.committee,
              address: event.address,
              errorCopy: context.errorCopy,
            })
          : {},
    }),
    trackTargetSubmitted: ({ context, event }) => {
      if (event.type !== "SUBMIT_TARGET") return;
      context.track(
        HIRING_EVENTS.targetSubmitted,
        { request: context.request, committee: event.committee },
        context.trackCtx,
      );
    },
    trackTargetInvalid: ({ context }) =>
      context.track(HIRING_EVENTS.targetInvalid, { request: context.request }, context.trackCtx),
    saveReasons: assign({
      draft: ({ context, event }) =>
        event.type === "SUBMIT_REASONS"
          ? {
              ...context.draft,
              reasons: event.reasons,
              evidence: event.evidence,
              coAuthors: event.coAuthors,
            }
          : context.draft,
      errors: () => ({}),
    }),
    setReasonsErrors: assign({
      errors: ({ context, event }) =>
        event.type === "SUBMIT_REASONS"
          ? validateReasons({
              reasons: event.reasons,
              evidence: event.evidence,
              errorCopy: context.errorCopy,
            })
          : {},
    }),
    trackReasonsSubmitted: ({ context, event }) => {
      if (event.type !== "SUBMIT_REASONS") return;
      context.track(
        HIRING_EVENTS.reasonsSubmitted,
        {
          request: context.request,
          reasons_length: event.reasons.trim().length,
          evidence_length: event.evidence.trim().length,
          co_authors: event.coAuthors.length,
        },
        context.trackCtx,
      );
    },
    trackReviewReached: ({ context }) =>
      context.track(HIRING_EVENTS.reviewReached, { request: context.request }, context.trackCtx),
    trackSubmitting: ({ context }) =>
      context.track(HIRING_EVENTS.submitting, { request: context.request }, context.trackCtx),
    trackSubmitted: ({ context }) =>
      context.track(
        HIRING_EVENTS.submitted,
        {
          request: context.request,
          proposal_id: context.result?.id,
        },
        context.trackCtx,
      ),
    trackSubmitError: ({ context }) =>
      context.track(
        HIRING_EVENTS.submitError,
        { request: context.request, error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "hiringSubmitWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    request: input.request,
    errorCopy: input.errorCopy,
    submit: input.submit ?? defaultSubmit,
    track: input.track ?? defaultTrack,
    draft: emptyDraft(),
    errors: {},
  }),
  initial: "target",
  states: {
    target: {
      entry: "trackStarted",
      on: {
        SUBMIT_TARGET: [
          {
            guard: "targetValid",
            target: "reasons",
            actions: ["saveTarget", "trackTargetSubmitted"],
          },
          {
            actions: ["setTargetErrors", "trackTargetInvalid"],
          },
        ],
      },
    },
    reasons: {
      on: {
        SUBMIT_REASONS: [
          {
            guard: "reasonsValid",
            target: "review",
            actions: ["saveReasons", "trackReasonsSubmitted"],
          },
          {
            actions: "setReasonsErrors",
          },
        ],
        BACK: { target: "target", actions: assign({ errors: () => ({}) }) },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        CONFIRM: { target: "submitting" },
        BACK: { target: "reasons" },
      },
    },
    submitting: {
      entry: ["trackSubmitting", assign({ error: undefined })],
      invoke: {
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({
          request: context.request,
          draft: context.draft,
          submit: context.submit,
        }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackSubmitted"],
        },
        onError: {
          target: "error",
          actions: [
            assign({
              error: ({ event }) =>
                toErrorMessage(event.error, "submit failed"),
            }),
            "trackSubmitError",
          ],
        },
      },
    },
    success: {
      type: "final",
    },
    error: {
      on: {
        RETRY: { target: "submitting" },
        BACK: { target: "review" },
      },
    },
  },
});

export type HiringMachine = typeof hiringMachine;

export function resolveHiringSnapshot(args: {
  step: HiringStateId;
  trackCtx: TrackContext;
  request: HiringRequest;
  errorCopy: ErrorCopy;
  submit?: SubmitFn;
  track?: TrackFn;
  draft?: HiringDraft;
}) {
  const { step, trackCtx, request, errorCopy, submit, track, draft } = args;
  if (step === "target") return undefined;
  const context: HiringContext = {
    trackCtx,
    request,
    errorCopy,
    submit: submit ?? defaultSubmit,
    track: track ?? defaultTrack,
    draft: draft ?? emptyDraft(),
    errors: {},
  };
  return hiringMachine.resolveState({ value: step, context });
}
