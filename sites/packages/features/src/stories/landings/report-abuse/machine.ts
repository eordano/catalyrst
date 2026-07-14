import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  isEthAddress,
  failClosedSubmitReport,
  validateDraft,
  invalidFields,
  type EvidenceUpload,
  type ReportDraft,
  type ReportReason,
  type SubmitReportFn,
  type SubmitReportResult,
} from "@data/lib/catalyst/landings/report";

export type { TrackFn };

export type ReportInput = {
  trackCtx: TrackContext;
  playerAddress?: string;
  submit?: SubmitReportFn;
  track?: TrackFn;
};

export type ReportContext = {
  trackCtx: TrackContext;
  submit: SubmitReportFn;
  track: TrackFn;
  draft: ReportDraft;
  result?: SubmitReportResult;
  error?: string;
};

export type ReportEvent =
  | { type: "START" }
  | { type: "SET_TARGET"; reportedAddress: string }
  | { type: "SET_CATEGORY"; reason: ReportReason }
  | { type: "SET_DETAILS"; description: string }
  | { type: "SET_EVIDENCE"; evidence: EvidenceUpload[] }
  | { type: "CONTINUE" }
  | { type: "SET_CONFIRM"; confirmAccuracy: boolean }
  | { type: "SUBMIT" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const REPORT_EVENTS = {
  started: "report_started",
  targetSet: "report_target_set",
  categorySet: "report_category_set",
  detailsSet: "report_details_set",
  evidenceAdded: "report_evidence_added",
  reviewReached: "report_review_reached",
  validationFailed: "report_validation_failed",
  submitStarted: "report_submit_started",
  completed: "report_completed",
  failed: "report_failed",
} as const;

export const STATE_TO_SLUG = {
  intro: "intro",
  target: "target",
  category: "category",
  details: "details",
  evidence: "evidence",
  review: "review",
  submitting: "submitting",
  success: "success",
  error: "error",
} as const;

export type ReportStateId = keyof typeof STATE_TO_SLUG;
export type ReportStepSlug = (typeof STATE_TO_SLUG)[ReportStateId];

export const FIRST_STEP_SLUG: ReportStepSlug = STATE_TO_SLUG.intro;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "intro");

export const SLUG_TO_STATE: Record<ReportStepSlug, ReportStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => ReportStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => ReportStateId = stepSlugs.toState;

export function emptyDraft(playerAddress = ""): ReportDraft {
  return {
    playerAddress,
    reportedAddress: "",
    reason: "",
    description: "",
    evidence: [],
    additionalComments: "",
    confirmAccuracy: false,
  };
}

export const reportMachine = setup({
  types: {
    context: {} as ReportContext,
    events: {} as ReportEvent,
    input: {} as ReportInput,
  },
  actors: {
    runSubmit: fromPromise<SubmitReportResult, { draft: ReportDraft; submit: SubmitReportFn }>(
      ({ input, signal }) => input.submit({ draft: input.draft, signal }),
    ),
  },
  guards: {
    targetValid: ({ event }) =>
      event.type === "SET_TARGET" && isEthAddress(event.reportedAddress),
    detailsValid: ({ event }) =>
      event.type === "SET_DETAILS" && event.description.trim().length > 0,
    hasEvidence: ({ context }) => context.draft.evidence.length > 0,
    isConfirmed: ({ context }) => context.draft.confirmAccuracy === true,
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(REPORT_EVENTS.started, {}, context.trackCtx),
    setTarget: assign({
      draft: ({ context, event }) =>
        event.type === "SET_TARGET"
          ? { ...context.draft, reportedAddress: event.reportedAddress.trim() }
          : context.draft,
    }),
    trackTargetSet: ({ context }) =>
      context.track(
        REPORT_EVENTS.targetSet,
        { has_reporter: isEthAddress(context.draft.playerAddress) },
        context.trackCtx,
      ),
    setCategory: assign({
      draft: ({ context, event }) =>
        event.type === "SET_CATEGORY"
          ? { ...context.draft, reason: event.reason }
          : context.draft,
    }),
    trackCategorySet: ({ context, event }) => {
      if (event.type !== "SET_CATEGORY") return;
      context.track(
        REPORT_EVENTS.categorySet,
        { reason: event.reason },
        context.trackCtx,
      );
    },
    setDetails: assign({
      draft: ({ context, event }) =>
        event.type === "SET_DETAILS"
          ? { ...context.draft, description: event.description }
          : context.draft,
    }),
    trackDetailsSet: ({ context, event }) => {
      if (event.type !== "SET_DETAILS") return;
      context.track(
        REPORT_EVENTS.detailsSet,
        { description_len: event.description.trim().length },
        context.trackCtx,
      );
    },
    setEvidence: assign({
      draft: ({ context, event }) =>
        event.type === "SET_EVIDENCE"
          ? { ...context.draft, evidence: event.evidence }
          : context.draft,
    }),
    trackEvidenceAdded: ({ context, event }) => {
      if (event.type !== "SET_EVIDENCE") return;
      context.track(
        REPORT_EVENTS.evidenceAdded,
        { file_count: event.evidence.length },
        context.trackCtx,
      );
    },
    setConfirm: assign({
      draft: ({ context, event }) =>
        event.type === "SET_CONFIRM"
          ? { ...context.draft, confirmAccuracy: event.confirmAccuracy }
          : context.draft,
    }),
    trackReviewReached: ({ context }) =>
      context.track(REPORT_EVENTS.reviewReached, {}, context.trackCtx),
    trackValidationFailedTarget: ({ context }) =>
      context.track(
        REPORT_EVENTS.validationFailed,
        { fields: ["reportedAddress"], step: "target" },
        context.trackCtx,
      ),
    trackValidationFailedDetails: ({ context }) =>
      context.track(
        REPORT_EVENTS.validationFailed,
        { fields: ["description"], step: "details" },
        context.trackCtx,
      ),
    trackValidationFailedEvidence: ({ context }) =>
      context.track(
        REPORT_EVENTS.validationFailed,
        { fields: ["evidence"], step: "evidence" },
        context.trackCtx,
      ),
    trackValidationFailedReview: ({ context }) =>
      context.track(
        REPORT_EVENTS.validationFailed,
        { fields: invalidFields(validateDraft(context.draft)), step: "review" },
        context.trackCtx,
      ),
    trackSubmitStarted: ({ context }) =>
      context.track(
        REPORT_EVENTS.submitStarted,
        { reason: context.draft.reason, evidence_count: context.draft.evidence.length },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        REPORT_EVENTS.completed,
        {
          reason: context.draft.reason,
          report_id: context.result?.reportId,
          evidence_count: context.draft.evidence.length,
        },
        context.trackCtx,
      ),
    trackFailed: ({ context }) =>
      context.track(
        REPORT_EVENTS.failed,
        { reason: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "reportWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    submit: input.submit ?? failClosedSubmitReport,
    track: input.track ?? defaultTrack,
    draft: emptyDraft(input.playerAddress ?? ""),
  }),
  initial: "intro",
  states: {
    intro: {
      on: {
        START: { target: "target", actions: "trackStarted" },
      },
    },
    target: {
      on: {
        SET_TARGET: [
          {
            guard: "targetValid",
            target: "category",
            actions: ["setTarget", "trackTargetSet"],
          },
          { actions: "trackValidationFailedTarget" },
        ],
        BACK: { target: "intro" },
      },
    },
    category: {
      on: {
        SET_CATEGORY: {
          target: "details",
          actions: ["setCategory", "trackCategorySet"],
        },
        BACK: { target: "target" },
      },
    },
    details: {
      on: {
        SET_DETAILS: [
          {
            guard: "detailsValid",
            target: "evidence",
            actions: ["setDetails", "trackDetailsSet"],
          },
          { actions: "trackValidationFailedDetails" },
        ],
        BACK: { target: "category" },
      },
    },
    evidence: {
      on: {
        SET_EVIDENCE: { actions: ["setEvidence", "trackEvidenceAdded"] },
        CONTINUE: [
          { guard: "hasEvidence", target: "review" },
          { actions: "trackValidationFailedEvidence" },
        ],
        BACK: { target: "details" },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        SET_CONFIRM: { actions: "setConfirm" },
        SUBMIT: [
          { guard: "isConfirmed", target: "submitting" },
          { actions: "trackValidationFailedReview" },
        ],
        BACK: { target: "evidence" },
      },
    },
    submitting: {
      entry: [assign({ error: undefined }), "trackSubmitStarted"],
      invoke: {
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({ draft: context.draft, submit: context.submit }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "report submit failed"),
          }),
        },
      },
    },
    success: {
      type: "final",
    },
    error: {
      entry: "trackFailed",
      on: {
        RETRY: { target: "submitting" },
      },
    },
  },
});

export type ReportMachine = typeof reportMachine;

export function resolveReportSnapshot(args: {
  step: ReportStateId;
  trackCtx: TrackContext;
  playerAddress?: string;
  seed?: Partial<ReportDraft>;
  submit?: SubmitReportFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, playerAddress = "", seed, submit, track } = args;
  if (step === "intro") return undefined;
  const draft: ReportDraft = { ...emptyDraft(playerAddress), ...(seed ?? {}) };
  const context: ReportContext = {
    trackCtx,
    submit: submit ?? failClosedSubmitReport,
    track: track ?? defaultTrack,
    draft,
  };
  return reportMachine.resolveState({ value: step, context });
}
