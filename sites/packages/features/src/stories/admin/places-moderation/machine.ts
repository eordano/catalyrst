import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  moderateReport,
  simulateModerateReport,
  type ModerationDecision,
  type ModerationResult,
  type ReportRow,
} from "@data/lib/catalyst/admin/places-moderation";

export type { TrackFn };

export type ModerateFn = (args: {
  report: ReportRow;
  decision: ModerationDecision;
  resolution?: string;
  notes?: string;
  disablePlace?: boolean;
  disableReason?: string;
  signal?: AbortSignal;
}) => Promise<ModerationResult>;

export type ModerateInput = {
  trackCtx: TrackContext;
  reports?: ReportRow[];
  queueOpenCount?: number;
  queueTotal?: number;
  moderate?: ModerateFn;
  track?: TrackFn;
};

export type ModerateContext = {
  trackCtx: TrackContext;
  reports: ReportRow[];
  queueOpenCount: number;
  queueTotal: number;
  moderate: ModerateFn;
  track: TrackFn;
  reportId?: string;
  decision?: ModerationDecision;
  resolution?: string;
  notes?: string;
  disablePlace?: boolean;
  result?: ModerationResult;
  error?: string;
};

export type ModerateEvent =
  | { type: "OPEN"; reportId: string }
  | { type: "CLOSE" }
  | { type: "DECIDE"; decision: ModerationDecision; resolution?: string; notes?: string }
  | { type: "TOGGLE_DISABLE"; disabled: boolean }
  | { type: "CANCEL" }
  | { type: "CONFIRM" }
  | { type: "RETRY" }
  | { type: "CONTINUE" };

export const MODERATE_EVENTS = {
  queueViewed: "admin_place_queue_viewed",
  reportOpened: "admin_place_report_opened",
  decisionSelected: "admin_place_decision_selected",
  disableToggled: "admin_place_disable_toggled",
  committed: "admin_place_moderation_committed",
  failed: "admin_place_moderation_failed",
} as const;

/**
 * There is no auth-gate step any more.
 *
 * It used to be the initial state, with an unconditional `SIGN_IN -> queue`
 * transition behind a button labelled "Open moderation console". Nothing was
 * checked: clicking it revealed the console to any anonymous visitor. That is
 * exactly the frontend-authorization theatre this surface must not have.
 *
 * Access is decided entirely server-side by
 * `catalyrst-places/src/handlers/admin.rs:41` -> `auth.rs:88-100`, and the
 * route loader reports that answer: `admin.places-moderation.tsx` renders this
 * wizard only when `loadReportQueue` came back ok, and renders the unavailable
 * reason otherwise. Entering `queue` therefore *is* the server's answer, not a
 * click.
 */
export const STATE_TO_SLUG = {
  queue: "queue",
  reviewReport: "review-report",
  decision: "decision",
  submitting: "submitting",
  moderated: "moderated",
} as const;

export type ModerateStateId = keyof typeof STATE_TO_SLUG;
export type ModerateStepSlug = (typeof STATE_TO_SLUG)[ModerateStateId];

export const FIRST_STEP_SLUG: ModerateStepSlug = STATE_TO_SLUG.queue;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "queue");

export const SLUG_TO_STATE: Record<ModerateStepSlug, ModerateStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => ModerateStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => ModerateStateId = stepSlugs.toState;

export const moderateDecision: ModerateFn = ({
  report,
  decision,
  resolution,
  notes,
  disablePlace,
  disableReason,
  signal,
}) =>
  moderateReport(
    { report, decision, resolution, notes, disablePlace, disableReason },
    { signal },
  );

export const simulateModerateDecision: ModerateFn = ({
  report,
  decision,
  resolution,
  notes,
  disablePlace,
  disableReason,
  signal,
}) =>
  simulateModerateReport(
    { report, decision, resolution, notes, disablePlace, disableReason },
    { signal },
  );

function placeholderReport(id: string): ReportRow {
  return {
    id,
    entity_id: null,
    reporter: "unknown",
    status: "open",
    reason: null,
    resolution: null,
    notes: null,
    resolved_by: null,
    resolved_at: null,
    created_at: new Date(0).toISOString(),
    place_title: null,
    place_coords: null,
    place_image: null,
    place_creator: null,
    payload: null,
  };
}

export function reportInContext(ctx: ModerateContext): ReportRow | undefined {
  if (!ctx.reportId) return undefined;
  return ctx.reports.find((r) => r.id === ctx.reportId) ?? placeholderReport(ctx.reportId);
}

export const moderateMachine = setup({
  types: {
    context: {} as ModerateContext,
    events: {} as ModerateEvent,
    input: {} as ModerateInput,
  },
  actors: {
    runModerate: fromPromise<
      ModerationResult,
      {
        report: ReportRow;
        decision: ModerationDecision;
        resolution?: string;
        notes?: string;
        disablePlace?: boolean;
        moderate: ModerateFn;
      }
    >(({ input, signal }) =>
      input.moderate({
        report: input.report,
        decision: input.decision,
        resolution: input.resolution,
        notes: input.notes,
        disablePlace: input.disablePlace,
        signal,
      }),
    ),
  },
  actions: {
    trackQueueViewed: ({ context }) =>
      context.track(
        MODERATE_EVENTS.queueViewed,
        { open_count: context.queueOpenCount, total: context.queueTotal },
        context.trackCtx,
      ),
    setReport: assign({
      reportId: ({ event }) => (event.type === "OPEN" ? event.reportId : undefined),
      decision: undefined,
      resolution: undefined,
      notes: undefined,
      disablePlace: undefined,
      result: undefined,
      error: undefined,
    }),
    trackReportOpened: ({ context, event }) => {
      if (event.type !== "OPEN") return;
      const report = context.reports.find((r) => r.id === event.reportId);
      context.track(
        MODERATE_EVENTS.reportOpened,
        { report_id: event.reportId, entity_id: report?.entity_id ?? null },
        context.trackCtx,
      );
    },
    setDecision: assign({
      decision: ({ event, context }) =>
        event.type === "DECIDE" ? event.decision : context.decision,
      resolution: ({ event, context }) =>
        event.type === "DECIDE" ? event.resolution : context.resolution,
      notes: ({ event, context }) =>
        event.type === "DECIDE" ? event.notes : context.notes,
      disablePlace: ({ event, context }) => {
        if (event.type !== "DECIDE") return context.disablePlace;
        return event.decision === "action" ? true : false;
      },
    }),
    trackDecisionSelected: ({ context, event }) => {
      if (event.type !== "DECIDE") return;
      context.track(
        MODERATE_EVENTS.decisionSelected,
        { report_id: context.reportId, decision: event.decision },
        context.trackCtx,
      );
    },
    setDisable: assign({
      disablePlace: ({ event, context }) =>
        event.type === "TOGGLE_DISABLE" ? event.disabled : context.disablePlace,
    }),
    trackDisableToggled: ({ context, event }) => {
      if (event.type !== "TOGGLE_DISABLE") return;
      const report = reportInContext(context);
      context.track(
        MODERATE_EVENTS.disableToggled,
        { place_id: report?.entity_id ?? null, disabled: event.disabled },
        context.trackCtx,
      );
    },
    trackCommitted: ({ context }) =>
      context.track(
        MODERATE_EVENTS.committed,
        {
          report_id: context.reportId,
          decision: context.decision,
          place_disabled: context.result?.placeDisabled ?? false,
        },
        context.trackCtx,
      ),
    trackFailed: ({ context }) =>
      context.track(
        MODERATE_EVENTS.failed,
        { report_id: context.reportId, reason: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "adminPlacesModeration",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    reports: input.reports ?? [],
    queueOpenCount: input.queueOpenCount ?? 0,
    queueTotal: input.queueTotal ?? 0,
    moderate: input.moderate ?? moderateDecision,
    track: input.track ?? defaultTrack,
  }),
  initial: "queue",
  states: {
    queue: {
      entry: "trackQueueViewed",
      on: {
        OPEN: { target: "reviewReport", actions: ["setReport", "trackReportOpened"] },
      },
    },
    reviewReport: {
      on: {
        DECIDE: { target: "decision", actions: ["setDecision", "trackDecisionSelected"] },
        CLOSE: { target: "queue" },
      },
    },
    decision: {
      on: {
        DECIDE: { actions: ["setDecision", "trackDecisionSelected"] },
        TOGGLE_DISABLE: { actions: ["setDisable", "trackDisableToggled"] },
        CONFIRM: {
          target: "submitting",
          actions: [assign({ error: undefined }), "trackCommitted"],
        },
        CANCEL: { target: "reviewReport", actions: assign({ error: undefined }) },
      },
    },
    submitting: {
      invoke: {
        id: "runModerate",
        src: "runModerate",
        input: ({ context }) => ({
          report: reportInContext(context) ?? placeholderReport(context.reportId ?? ""),
          decision: context.decision ?? "resolve",
          resolution: context.resolution,
          notes: context.notes,
          disablePlace: context.disablePlace,
          moderate: context.moderate,
        }),
        onDone: {
          target: "moderated",
          actions: assign({ result: ({ event }) => event.output }),
        },
        onError: {
          target: "decision",
          actions: [
            assign({
              error: ({ event }) =>
                toErrorMessage(event.error, "moderation failed"),
            }),
            "trackFailed",
          ],
        },
      },
    },
    moderated: {
      on: {
        CONTINUE: {
          target: "queue",
          actions: assign({
            reportId: undefined,
            decision: undefined,
            resolution: undefined,
            notes: undefined,
            disablePlace: undefined,
            result: undefined,
            error: undefined,
          }),
        },
      },
    },
  },
});

export type ModerateMachine = typeof moderateMachine;

export function resolveModerateSnapshot(args: {
  step: ModerateStateId;
  trackCtx: TrackContext;
  reports?: ReportRow[];
  queueOpenCount?: number;
  queueTotal?: number;
  moderate?: ModerateFn;
  track?: TrackFn;
  reportId?: string;
  decision?: ModerationDecision;
}) {
  const { step, trackCtx, reports = [], moderate, track, reportId, decision } = args;
  if (step === "queue") return undefined;
  const seededDecision =
    step === "decision" || step === "submitting" || step === "moderated"
      ? (decision ?? "resolve")
      : undefined;
  const context: ModerateContext = {
    trackCtx,
    reports,
    queueOpenCount: args.queueOpenCount ?? 0,
    queueTotal: args.queueTotal ?? 0,
    moderate: moderate ?? moderateDecision,
    track: track ?? defaultTrack,
    reportId,
    decision: seededDecision,
    disablePlace: seededDecision === "action" ? true : undefined,
  };
  return moderateMachine.resolveState({ value: step, context });
}
