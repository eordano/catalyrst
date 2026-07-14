import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  GOVERNANCE_SCHEMA,
  validateDetails,
  failClosedCreateProposal,
  type FieldErrors,
  type GovernanceProposalDraft,
  type CreatedProposal,
} from "@data/lib/catalyst/governance/submit-governance-proposal";

export type { TrackFn };

export type GovDraft = GovernanceProposalDraft;

export type SubmitResult = CreatedProposal;

export type SubmitFn = (args: {
  draft: GovDraft;
  signal?: AbortSignal;
}) => Promise<SubmitResult>;

export type GovInput = {
  trackCtx: TrackContext;
  votingPower: number;
  submit?: SubmitFn;
  track?: TrackFn;
  draft?: Partial<GovDraft>;
};

export type GovContext = {
  trackCtx: TrackContext;
  votingPower: number;
  submit: SubmitFn;
  track: TrackFn;
  draft: GovDraft;
  errors: FieldErrors;
  result?: SubmitResult;
  error?: string;
};

export type GovEvent =
  | { type: "START" }
  | { type: "SUBMIT_DETAILS"; linkedDraftId: string; title: string; bodies: Record<string, string> }
  | { type: "SET_COAUTHORS"; coAuthors: string[] }
  | { type: "NEXT" }
  | { type: "BACK" }
  | { type: "SUBMIT" }
  | { type: "RETRY" };

export const GOVPROP_EVENTS = {
  started: "gv_govprop_started",
  vpBlocked: "gv_govprop_vp_blocked",
  detailsSubmitted: "gv_govprop_details_submitted",
  detailsInvalid: "gv_govprop_details_invalid",
  stepAdvanced: "gv_govprop_step_advanced",
  submitAttempted: "gv_govprop_submit_attempted",
  submitted: "gv_govprop_submitted",
  error: "gv_govprop_error",
} as const;

export const STATE_TO_SLUG = {
  intro: "intro",
  details: "details",
  coauthors: "coauthors",
  review: "review",
  submitting: "submitting",
  submitError: "submit-error",
  success: "success",
} as const;

export type GovStateId = keyof typeof STATE_TO_SLUG;
export type GovStepSlug = (typeof STATE_TO_SLUG)[GovStateId];

export const FIRST_STEP_SLUG: GovStepSlug = STATE_TO_SLUG.intro;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "intro");

export const SLUG_TO_STATE: Record<GovStepSlug, GovStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => GovStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => GovStateId = stepSlugs.toState;

export function emptyDraft(): GovDraft {
  return { linkedDraftId: "", title: "", bodies: {}, coAuthors: [] };
}

export const failClosedSubmit: SubmitFn = (args) => failClosedCreateProposal(args);

export const govProposalMachine = setup({
  types: {
    context: {} as GovContext,
    events: {} as GovEvent,
    input: {} as GovInput,
  },
  actors: {
    runSubmit: fromPromise<SubmitResult, { draft: GovDraft; submit: SubmitFn }>(
      ({ input, signal }) => input.submit({ draft: input.draft, signal }),
    ),
  },
  guards: {
    vpMet: ({ context }) => context.votingPower >= GOVERNANCE_SCHEMA.vpThreshold,
    detailsValid: ({ event }) => {
      if (event.type !== "SUBMIT_DETAILS") return false;
      return (
        Object.keys(
          validateDetails({
            linkedDraftId: event.linkedDraftId,
            title: event.title,
            bodies: event.bodies,
          }),
        ).length === 0
      );
    },
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(
        GOVPROP_EVENTS.started,
        { linked_draft_id: context.draft.linkedDraftId, vp: context.votingPower },
        context.trackCtx,
      ),
    trackVpBlocked: ({ context }) =>
      context.track(
        GOVPROP_EVENTS.vpBlocked,
        { vp: context.votingPower, threshold: GOVERNANCE_SCHEMA.vpThreshold },
        context.trackCtx,
      ),
    saveDetails: assign({
      draft: ({ context, event }) =>
        event.type === "SUBMIT_DETAILS"
          ? {
              ...context.draft,
              linkedDraftId: event.linkedDraftId,
              title: event.title,
              bodies: event.bodies,
            }
          : context.draft,
      errors: () => ({}),
    }),
    setDetailsErrors: assign({
      errors: ({ event }) =>
        event.type === "SUBMIT_DETAILS"
          ? validateDetails({
              linkedDraftId: event.linkedDraftId,
              title: event.title,
              bodies: event.bodies,
            })
          : {},
    }),
    trackDetailsSubmitted: ({ context, event }) => {
      if (event.type !== "SUBMIT_DETAILS") return;
      const filled = Object.values(event.bodies).filter(
        (v) => (v ?? "").trim().length > 0,
      ).length;
      context.track(
        GOVPROP_EVENTS.detailsSubmitted,
        { title_len: event.title.trim().length, bodies_filled: filled },
        context.trackCtx,
      );
    },
    trackDetailsInvalid: ({ context, event }) => {
      if (event.type !== "SUBMIT_DETAILS") return;
      const errs = validateDetails({
        linkedDraftId: event.linkedDraftId,
        title: event.title,
        bodies: event.bodies,
      });
      context.track(
        GOVPROP_EVENTS.detailsInvalid,
        { error_count: Object.keys(errs).length },
        context.trackCtx,
      );
    },
    saveCoAuthors: assign({
      draft: ({ context, event }) =>
        event.type === "SET_COAUTHORS"
          ? { ...context.draft, coAuthors: event.coAuthors }
          : context.draft,
    }),
    trackStepReview: ({ context }) =>
      context.track(
        GOVPROP_EVENTS.stepAdvanced,
        { to: "review", co_authors: context.draft.coAuthors.length },
        context.trackCtx,
      ),
    trackSubmitAttempted: ({ context }) =>
      context.track(
        GOVPROP_EVENTS.submitAttempted,
        { linked_draft_id: context.draft.linkedDraftId, title_len: context.draft.title.length },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        GOVPROP_EVENTS.submitted,
        { proposal_id: context.result?.id },
        context.trackCtx,
      ),
    trackError: ({ context }) =>
      context.track(
        GOVPROP_EVENTS.error,
        { error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "govProposalSubmit",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    votingPower: input.votingPower,
    submit: input.submit ?? failClosedSubmit,
    track: input.track ?? defaultTrack,
    draft: { ...emptyDraft(), ...(input.draft ?? {}) },
    errors: {},
  }),
  initial: "intro",
  states: {
    intro: {
      on: {
        START: [
          { guard: "vpMet", target: "details", actions: "trackStarted" },
          { actions: "trackVpBlocked" },
        ],
        SET_COAUTHORS: { actions: "saveCoAuthors" },
        SUBMIT_DETAILS: { actions: "saveDetails" },
      },
    },
    details: {
      on: {
        SUBMIT_DETAILS: [
          {
            guard: "detailsValid",
            target: "coauthors",
            actions: ["saveDetails", "trackDetailsSubmitted"],
          },
          {
            actions: ["setDetailsErrors", "trackDetailsInvalid"],
          },
        ],
        BACK: { target: "intro", actions: assign({ errors: () => ({}) }) },
      },
    },
    coauthors: {
      on: {
        SET_COAUTHORS: { actions: "saveCoAuthors" },
        NEXT: { target: "review", actions: "trackStepReview" },
        BACK: { target: "details" },
      },
    },
    review: {
      on: {
        SUBMIT: { target: "submitting", actions: "trackSubmitAttempted" },
        BACK: { target: "coauthors" },
      },
    },
    submitting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({ draft: context.draft, submit: context.submit }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackSubmitted"],
        },
        onError: {
          target: "submitError",
          actions: [
            assign({
              error: ({ event }) =>
                toErrorMessage(event.error, "submit failed"),
            }),
            "trackError",
          ],
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

export type GovProposalMachine = typeof govProposalMachine;

export function resolveGovProposalSnapshot(args: {
  step: GovStateId;
  trackCtx: TrackContext;
  votingPower: number;
  submit?: SubmitFn;
  track?: TrackFn;
  draft?: Partial<GovDraft>;
}) {
  const { step, trackCtx, votingPower, submit, track, draft } = args;
  if (step === "intro") return undefined;
  const sampleBodies: Record<string, string> = {};
  for (const b of GOVERNANCE_SCHEMA.bodies) {
    sampleBodies[b.name] = `Sample ${b.label.toLowerCase()} content for the linked Draft proposal.`;
  }
  const seeded: GovDraft = {
    linkedDraftId: "9d0f5b6f-1f47-4371-8a30-4ee99e3792ef",
    title: "Formalize: Reduce the VP threshold for Governance Proposals",
    bodies: sampleBodies,
    coAuthors: [],
    ...(draft ?? {}),
  };
  const context: GovContext = {
    trackCtx,
    votingPower,
    submit: submit ?? failClosedSubmit,
    track: track ?? defaultTrack,
    draft: seeded,
    errors: {},
  };
  return govProposalMachine.resolveState({ value: step, context });
}
