import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  emptyDraft,
  isStepValid,
  simulateSubmitSchedule,
  type ScheduleDraft,
  type SubmitResult,
} from "@data/lib/catalyst/landings/schedules";

export type { TrackFn };

export type SubmitFn = (args: {
  draft: ScheduleDraft;
  scheduleId?: string;
  signal?: AbortSignal;
}) => Promise<SubmitResult>;

export type ScheduleInput = {
  trackCtx: TrackContext;
  draft?: ScheduleDraft;
  scheduleId?: string;
  submit?: SubmitFn;
  track?: TrackFn;
};

export type ScheduleContext = {
  trackCtx: TrackContext;
  draft: ScheduleDraft;
  scheduleId?: string;
  submit: SubmitFn;
  track: TrackFn;
  result?: SubmitResult;
  error?: string;
};

export type DraftPatch = Partial<ScheduleDraft>;

export type ScheduleEvent =
  | { type: "SIGN_IN" }
  | { type: "EDIT"; patch: DraftPatch }
  | { type: "NEXT" }
  | { type: "BACK" }
  | { type: "SUBMIT" }
  | { type: "RETRY" };

export const SCHEDULE_EVENTS = {
  listViewed: "lp_schedule_list_viewed",
  gateViewed: "lp_schedule_gate_viewed",
  started: "lp_schedule_started",
  stepCompleted: "lp_schedule_step_completed",
  reviewReached: "lp_schedule_review_reached",
  submitAttempted: "lp_schedule_submit_attempted",
  submitFailed: "lp_schedule_submit_failed",
  created: "lp_schedule_created",
} as const;

export const STATE_TO_SLUG = {
  authGate: "auth-gate",
  basics: "basics",
  dates: "dates",
  review: "review",
  submitting: "submitting",
  created: "created",
} as const;

export type ScheduleStateId = keyof typeof STATE_TO_SLUG;
export type ScheduleStepSlug = (typeof STATE_TO_SLUG)[ScheduleStateId];

export const FIRST_STEP_SLUG: ScheduleStepSlug = STATE_TO_SLUG.authGate;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "authGate");

export const SLUG_TO_STATE: Record<ScheduleStepSlug, ScheduleStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => ScheduleStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => ScheduleStateId = stepSlugs.toState;

export const FORM_ORDER: ScheduleStateId[] = ["basics", "dates", "review"];

export const simulateSubmit: SubmitFn = ({ draft, scheduleId, signal }) =>
  simulateSubmitSchedule(draft, { scheduleId, signal });

export const scheduleMachine = setup({
  types: {
    context: {} as ScheduleContext,
    events: {} as ScheduleEvent,
    input: {} as ScheduleInput,
  },
  actors: {
    runSubmit: fromPromise<
      SubmitResult,
      { draft: ScheduleDraft; scheduleId?: string; submit: SubmitFn }
    >(({ input, signal }) =>
      input.submit({ draft: input.draft, scheduleId: input.scheduleId, signal }),
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
      context.track(SCHEDULE_EVENTS.gateViewed, {}, context.trackCtx),
    trackStarted: ({ context }) =>
      context.track(
        SCHEDULE_EVENTS.started,
        { editing: Boolean(context.scheduleId) },
        context.trackCtx,
      ),
    trackReviewReached: ({ context }) =>
      context.track(SCHEDULE_EVENTS.reviewReached, {}, context.trackCtx),
    trackSubmitAttempted: ({ context }) =>
      context.track(
        SCHEDULE_EVENTS.submitAttempted,
        { editing: Boolean(context.scheduleId), theme: context.draft.theme || null },
        context.trackCtx,
      ),
    trackSubmitFailed: ({ context }) =>
      context.track(
        SCHEDULE_EVENTS.submitFailed,
        { error: context.error },
        context.trackCtx,
      ),
    trackCreated: ({ context }) =>
      context.track(
        SCHEDULE_EVENTS.created,
        {
          schedule_id: context.result?.id,
          active: context.result?.active,
          editing: Boolean(context.scheduleId),
          stub: true,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "eventSchedule",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    draft: input.draft ?? emptyDraft(),
    scheduleId: input.scheduleId,
    submit: input.submit ?? simulateSubmit,
    track: input.track ?? defaultTrack,
  }),
  initial: "authGate",
  on: {
    EDIT: { actions: "applyEdit" },
  },
  states: {
    authGate: {
      entry: "trackGateViewed",
      on: {
        SIGN_IN: { target: "basics", actions: "trackStarted" },
      },
    },
    basics: {
      on: {
        NEXT: {
          target: "dates",
          guard: { type: "stepValid", params: { step: "basics" } },
        },
        BACK: { target: "authGate" },
      },
    },
    dates: {
      on: {
        NEXT: {
          target: "review",
          guard: { type: "stepValid", params: { step: "dates" } },
        },
        BACK: { target: "basics" },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        SUBMIT: { target: "submitting", actions: "trackSubmitAttempted" },
        BACK: { target: "dates" },
      },
    },
    submitting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({
          draft: context.draft,
          scheduleId: context.scheduleId,
          submit: context.submit,
        }),
        onDone: {
          target: "created",
          actions: [assign({ result: ({ event }) => event.output }), "trackCreated"],
        },
        onError: {
          target: "review",
          actions: [
            assign({
              error: ({ event }) =>
                toErrorMessage(event.error, "submit failed"),
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

export type ScheduleMachine = typeof scheduleMachine;

export function emitStepCompleted(
  track: TrackFn,
  ctx: TrackContext,
  from: ScheduleStateId,
  to: ScheduleStateId,
): void {
  track(SCHEDULE_EVENTS.stepCompleted, { from, to }, ctx);
}

export function resolveScheduleSnapshot(args: {
  step: ScheduleStateId;
  trackCtx: TrackContext;
  draft?: ScheduleDraft;
  scheduleId?: string;
  submit?: SubmitFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, draft, scheduleId, submit, track } = args;
  if (step === "authGate") return undefined;
  const context: ScheduleContext = {
    trackCtx,
    draft: draft ?? emptyDraft(),
    scheduleId,
    submit: submit ?? simulateSubmit,
    track: track ?? defaultTrack,
  };
  return scheduleMachine.resolveState({ value: step, context });
}
