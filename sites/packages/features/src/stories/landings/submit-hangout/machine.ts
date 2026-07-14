import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  emptyDraft,
  isStepValid,
  simulateSubmitHangout,
  type HangoutDraft,
  type SubmitResult,
} from "@data/lib/catalyst/landings/submit-hangout";

export type { TrackFn };

export type SubmitFn = (args: {
  draft: HangoutDraft;
  signal?: AbortSignal;
}) => Promise<SubmitResult>;

export type HangoutInput = {
  trackCtx: TrackContext;
  draft?: HangoutDraft;
  submit?: SubmitFn;
  track?: TrackFn;
};

export type HangoutContext = {
  trackCtx: TrackContext;
  draft: HangoutDraft;
  submit: SubmitFn;
  track: TrackFn;
  result?: SubmitResult;
  error?: string;
};

export type DraftPatch = Partial<HangoutDraft>;

export type HangoutEvent =
  | { type: "SIGN_IN" }
  | { type: "EDIT"; patch: DraftPatch }
  | { type: "NEXT" }
  | { type: "BACK" }
  | { type: "PREVIEW" }
  | { type: "SUBMIT" }
  | { type: "RETRY" };

export const HANGOUT_EVENTS = {
  gateViewed: "lp_hangout_signin_gate_viewed",
  started: "lp_hangout_started",
  stepCompleted: "lp_hangout_step_completed",
  previewOpened: "lp_hangout_preview_opened",
  submitAttempted: "lp_hangout_submit_attempted",
  submitFailed: "lp_hangout_submit_failed",
  submitted: "lp_hangout_submitted",
} as const;

export const STATE_TO_SLUG = {
  signinGate: "signin-gate",
  cover: "cover",
  details: "details",
  location: "location",
  schedule: "schedule",
  review: "review",
  preview: "preview",
  submitting: "submitting",
  submitted: "submitted",
} as const;

export type HangoutStateId = keyof typeof STATE_TO_SLUG;
export type HangoutStepSlug = (typeof STATE_TO_SLUG)[HangoutStateId];

export const FIRST_STEP_SLUG: HangoutStepSlug = STATE_TO_SLUG.signinGate;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "signinGate");

export const SLUG_TO_STATE: Record<HangoutStepSlug, HangoutStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => HangoutStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => HangoutStateId = stepSlugs.toState;

export const FORM_ORDER: HangoutStateId[] = [
  "cover",
  "details",
  "location",
  "schedule",
  "review",
];

export const simulateSubmit: SubmitFn = ({ draft, signal }) =>
  simulateSubmitHangout(draft, { signal });

export const hangoutMachine = setup({
  types: {
    context: {} as HangoutContext,
    events: {} as HangoutEvent,
    input: {} as HangoutInput,
  },
  actors: {
    runSubmit: fromPromise<SubmitResult, { draft: HangoutDraft; submit: SubmitFn }>(
      ({ input, signal }) => input.submit({ draft: input.draft, signal }),
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
      context.track(HANGOUT_EVENTS.gateViewed, {}, context.trackCtx),
    trackStarted: ({ context }) =>
      context.track(HANGOUT_EVENTS.started, {}, context.trackCtx),
    trackPreviewOpened: ({ context }) =>
      context.track(HANGOUT_EVENTS.previewOpened, {}, context.trackCtx),
    trackSubmitAttempted: ({ context }) =>
      context.track(
        HANGOUT_EVENTS.submitAttempted,
        { recurrent: context.draft.recurrent, location: context.draft.location },
        context.trackCtx,
      ),
    trackSubmitFailed: ({ context }) =>
      context.track(
        HANGOUT_EVENTS.submitFailed,
        { error: context.error },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        HANGOUT_EVENTS.submitted,
        { event_id: context.result?.id, approved: context.result?.approved, stub: true },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "submitHangout",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    draft: input.draft ?? emptyDraft(),
    submit: input.submit ?? simulateSubmit,
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
        SIGN_IN: { target: "cover", actions: "trackStarted" },
      },
    },
    cover: {
      on: {
        NEXT: { target: "details" },
        BACK: { target: "signinGate" },
      },
    },
    details: {
      on: {
        NEXT: {
          target: "location",
          guard: { type: "stepValid", params: { step: "details" } },
        },
        BACK: { target: "cover" },
      },
    },
    location: {
      on: {
        NEXT: {
          target: "schedule",
          guard: { type: "stepValid", params: { step: "location" } },
        },
        BACK: { target: "details" },
      },
    },
    schedule: {
      on: {
        NEXT: {
          target: "review",
          guard: { type: "stepValid", params: { step: "schedule" } },
        },
        BACK: { target: "location" },
      },
    },
    review: {
      on: {
        PREVIEW: { target: "preview", actions: "trackPreviewOpened" },
        SUBMIT: {
          target: "submitting",
          actions: [assign({ error: undefined }), "trackSubmitAttempted"],
        },
        BACK: { target: "schedule" },
      },
    },
    preview: {
      on: {
        BACK: { target: "review" },
        SUBMIT: {
          target: "submitting",
          actions: [assign({ error: undefined }), "trackSubmitAttempted"],
        },
      },
    },
    submitting: {
      invoke: {
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({ draft: context.draft, submit: context.submit }),
        onDone: {
          target: "submitted",
          actions: [assign({ result: ({ event }) => event.output }), "trackSubmitted"],
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
    submitted: {
      type: "final",
    },
  },
});

export type HangoutMachine = typeof hangoutMachine;

export function emitStepCompleted(
  track: TrackFn,
  ctx: TrackContext,
  from: HangoutStateId,
  to: HangoutStateId,
): void {
  track(HANGOUT_EVENTS.stepCompleted, { from, to }, ctx);
}

export function resolveHangoutSnapshot(args: {
  step: HangoutStateId;
  trackCtx: TrackContext;
  draft?: HangoutDraft;
  submit?: SubmitFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, draft, submit, track } = args;
  if (step === "signinGate") return undefined;
  const context: HangoutContext = {
    trackCtx,
    draft: draft ?? emptyDraft(),
    submit: submit ?? simulateSubmit,
    track: track ?? defaultTrack,
  };
  return hangoutMachine.resolveState({ value: step, context });
}
