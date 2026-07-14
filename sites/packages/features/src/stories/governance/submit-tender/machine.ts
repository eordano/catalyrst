import { assign, fromPromise, setup } from "xstate";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  emptyTenderForm,
  isSubmissionVpNotMet,
  failClosedCreateTender,
  type CreatedTender,
  type TenderForm,
} from "@data/lib/catalyst/governance/submit-tender";

export type { TrackFn };

export type SubmitFn = (args: {
  form: TenderForm;
  signal?: AbortSignal;
}) => Promise<CreatedTender>;

export type TenderSeed = {
  votingPower: number;
  threshold: number;
  linkedProposalId: string;
};

export type TenderInput = {
  trackCtx: TrackContext;
  seed: TenderSeed;
  form?: Partial<TenderForm>;
  submit?: SubmitFn;
  track?: TrackFn;
};

export type TenderContext = {
  trackCtx: TrackContext;
  seed: TenderSeed;
  submit: SubmitFn;
  track: TrackFn;
  form: TenderForm;
  result?: CreatedTender;
  error?: string;
};

export type TenderEvent =
  | { type: "START" }
  | { type: "GATE" }
  | { type: "SET_FORM"; patch: Partial<TenderForm> }
  | { type: "NEXT" }
  | { type: "SUBMIT" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const TENDER_EVENTS = {
  started: "gv_tender_started",
  vpGated: "gv_tender_vp_gated",
  detailsFilled: "gv_tender_details_filled",
  coauthorsSet: "gv_tender_coauthors_set",
  reviewReached: "gv_tender_review_reached",
  submitting: "gv_tender_submitting",
  submitted: "gv_tender_submitted",
  submitError: "gv_tender_submit_error",
} as const;

export const STATE_TO_SLUG = {
  parent: "parent",
  gated: "gated",
  details: "details",
  coauthors: "coauthors",
  review: "review",
  submitting: "submitting",
  success: "success",
  error: "error",
} as const;

export type TenderStateId = keyof typeof STATE_TO_SLUG;
export type TenderStepSlug = (typeof STATE_TO_SLUG)[TenderStateId];

export const FIRST_STEP_SLUG: TenderStepSlug = STATE_TO_SLUG.parent;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "parent");

export const SLUG_TO_STATE: Record<TenderStepSlug, TenderStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => TenderStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => TenderStateId = stepSlugs.toState;

export const tenderMachine = setup({
  types: {
    context: {} as TenderContext,
    events: {} as TenderEvent,
    input: {} as TenderInput,
  },
  actors: {
    runSubmit: fromPromise<CreatedTender, { form: TenderForm; submit: SubmitFn }>(
      ({ input, signal }) => input.submit({ form: input.form, signal }),
    ),
  },
  guards: {
    vpMet: ({ context }) =>
      !isSubmissionVpNotMet(context.seed.votingPower, context.seed.threshold),
  },
  actions: {
    patchForm: assign({
      form: ({ context, event }) =>
        event.type === "SET_FORM" ? { ...context.form, ...event.patch } : context.form,
    }),
    trackStarted: ({ context }) =>
      context.track(
        TENDER_EVENTS.started,
        {
          linked_proposal_id: context.form.linked_proposal_id,
          voting_power: context.seed.votingPower,
        },
        context.trackCtx,
      ),
    trackVpGated: ({ context }) =>
      context.track(
        TENDER_EVENTS.vpGated,
        { voting_power: context.seed.votingPower, threshold: context.seed.threshold },
        context.trackCtx,
      ),
    trackDetailsFilled: ({ context }) =>
      context.track(
        TENDER_EVENTS.detailsFilled,
        {
          linked_proposal_id: context.form.linked_proposal_id,
          summary_len: context.form.summary.length,
          target_release_quarter: context.form.target_release_quarter,
        },
        context.trackCtx,
      ),
    trackCoauthorsSet: ({ context }) =>
      context.track(
        TENDER_EVENTS.coauthorsSet,
        { count: context.form.coAuthors.length },
        context.trackCtx,
      ),
    trackReviewReached: ({ context }) =>
      context.track(
        TENDER_EVENTS.reviewReached,
        { linked_proposal_id: context.form.linked_proposal_id },
        context.trackCtx,
      ),
    trackSubmitting: ({ context }) =>
      context.track(
        TENDER_EVENTS.submitting,
        { linked_proposal_id: context.form.linked_proposal_id },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        TENDER_EVENTS.submitted,
        {
          linked_proposal_id: context.form.linked_proposal_id,
          proposal_id: context.result?.id,
          pending: context.result?.pending ?? false,
        },
        context.trackCtx,
      ),
    trackSubmitError: ({ context }) =>
      context.track(
        TENDER_EVENTS.submitError,
        { linked_proposal_id: context.form.linked_proposal_id },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "submitTender",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    seed: input.seed,
    submit: input.submit ?? failClosedCreateTender,
    track: input.track ?? defaultTrack,
    form: { ...emptyTenderForm(input.seed.linkedProposalId), ...input.form },
  }),
  initial: "parent",
  states: {
    parent: {
      on: {
        START: [
          { guard: "vpMet", target: "details", actions: "trackStarted" },
          { target: "gated", actions: "trackVpGated" },
        ],
        GATE: { target: "gated", actions: "trackVpGated" },
        SET_FORM: { actions: "patchForm" },
      },
    },
    gated: {
      on: {
        RETRY: { target: "parent" },
      },
    },
    details: {
      on: {
        SET_FORM: { actions: "patchForm" },
        NEXT: { target: "coauthors", actions: "trackDetailsFilled" },
        BACK: { target: "parent" },
      },
    },
    coauthors: {
      on: {
        SET_FORM: { actions: "patchForm" },
        NEXT: { target: "review", actions: "trackCoauthorsSet" },
        BACK: { target: "details" },
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
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({ form: context.form, submit: context.submit }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackSubmitted"],
        },
        onError: {
          target: "error",
          actions: [
            assign({
              error: ({ event }) =>
                event.error instanceof Error ? event.error.message : "submit failed",
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

export type TenderMachine = typeof tenderMachine;

export function resolveTenderSnapshot(args: {
  step: TenderStateId;
  trackCtx: TrackContext;
  seed: TenderSeed;
  form?: Partial<TenderForm>;
  submit?: SubmitFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, seed, form, submit, track } = args;
  if (step === "parent") return undefined;
  const context: TenderContext = {
    trackCtx,
    seed,
    submit: submit ?? failClosedCreateTender,
    track: track ?? defaultTrack,
    form: { ...emptyTenderForm(seed.linkedProposalId), ...form },
  };
  return tenderMachine.resolveState({ value: step, context });
}
