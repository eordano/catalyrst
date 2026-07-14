import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  validateDetails,
  MARKDOWN_FIELDS,
  type FieldErrors,
  type PitchDetails,
} from "@data/lib/catalyst/governance/submit-pitch";

export type { TrackFn };

export type PitchDraft = PitchDetails & {
  coAuthors: string[];
};

export type SubmitResult = { proposalId: string; stub: true };

export type SubmitFn = (args: {
  draft: PitchDraft;
  signal?: AbortSignal;
}) => Promise<SubmitResult>;

export type PitchInput = {
  trackCtx: TrackContext;
  meetsGate: boolean;
  votingPower: number;
  submit?: SubmitFn;
  track?: TrackFn;
};

export type PitchContext = {
  trackCtx: TrackContext;
  meetsGate: boolean;
  votingPower: number;
  submit: SubmitFn;
  track: TrackFn;
  draft: PitchDraft;
  errors: FieldErrors;
  result?: SubmitResult;
  error?: string;
};

export type PitchEvent =
  | { type: "PASS_GATE" }
  | { type: "SUBMIT_DETAILS"; details: PitchDetails }
  | { type: "SUBMIT_COAUTHORS"; coAuthors: string[] }
  | { type: "CONFIRM" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const PITCH_EVENTS = {
  started: "gv_pitch_started",
  gatePassed: "gv_pitch_gate_passed",
  detailsSubmitted: "gv_pitch_details_submitted",
  detailsInvalid: "gv_pitch_details_invalid",
  coauthorsSet: "gv_pitch_coauthors_set",
  reviewReached: "gv_pitch_review_reached",
  submitting: "gv_pitch_submitting",
  submitted: "gv_pitch_submitted",
  error: "gv_pitch_error",
} as const;

export const STATE_TO_SLUG = {
  intro: "intro",
  details: "details",
  coauthors: "coauthors",
  review: "review",
  submitting: "submitting",
  success: "success",
  error: "error",
} as const;

export type PitchStateId = keyof typeof STATE_TO_SLUG;
export type PitchStepSlug = (typeof STATE_TO_SLUG)[PitchStateId];

export const FIRST_STEP_SLUG: PitchStepSlug = STATE_TO_SLUG.intro;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "intro");

export const SLUG_TO_STATE: Record<PitchStepSlug, PitchStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => PitchStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => PitchStateId = stepSlugs.toState;

export function emptyDraft(): PitchDraft {
  return {
    initiative_name: "",
    problem_statement: "",
    proposed_solution: "",
    target_audience: "",
    relevance: "",
    coAuthors: [],
  };
}

function bodyChars(details: PitchDetails): number {
  return MARKDOWN_FIELDS.reduce((sum, f) => sum + details[f].trim().length, 0);
}

export const simulateSubmit: SubmitFn = async ({ signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const proposalId = `sim-pitch-${Date.now().toString(36)}`;
  return { proposalId, stub: true };
};

export const pitchMachine = setup({
  types: {
    context: {} as PitchContext,
    events: {} as PitchEvent,
    input: {} as PitchInput,
  },
  actors: {
    runSubmit: fromPromise<SubmitResult, { draft: PitchDraft; submit: SubmitFn }>(
      ({ input, signal }) => input.submit({ draft: input.draft, signal }),
    ),
  },
  guards: {
    meetsGate: ({ context }) => context.meetsGate,
    detailsValid: ({ event }) => {
      if (event.type !== "SUBMIT_DETAILS") return false;
      return Object.keys(validateDetails(event.details)).length === 0;
    },
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(
        PITCH_EVENTS.started,
        { meets_gate: context.meetsGate, vp: context.votingPower },
        context.trackCtx,
      ),
    trackGatePassed: ({ context }) =>
      context.track(PITCH_EVENTS.gatePassed, { vp: context.votingPower }, context.trackCtx),
    saveDetails: assign({
      draft: ({ context, event }) =>
        event.type === "SUBMIT_DETAILS"
          ? { ...context.draft, ...event.details }
          : context.draft,
      errors: () => ({}),
    }),
    setDetailErrors: assign({
      errors: ({ event }) =>
        event.type === "SUBMIT_DETAILS" ? validateDetails(event.details) : {},
    }),
    trackDetailsSubmitted: ({ context, event }) => {
      if (event.type !== "SUBMIT_DETAILS") return;
      context.track(
        PITCH_EVENTS.detailsSubmitted,
        {
          name_length: event.details.initiative_name.trim().length,
          body_chars: bodyChars(event.details),
        },
        context.trackCtx,
      );
    },
    trackDetailsInvalid: ({ context, event }) => {
      if (event.type !== "SUBMIT_DETAILS") return;
      const fields = Object.keys(validateDetails(event.details));
      context.track(PITCH_EVENTS.detailsInvalid, { fields }, context.trackCtx);
    },
    saveCoAuthors: assign({
      draft: ({ context, event }) =>
        event.type === "SUBMIT_COAUTHORS"
          ? { ...context.draft, coAuthors: event.coAuthors }
          : context.draft,
    }),
    trackCoAuthorsSet: ({ context, event }) => {
      if (event.type !== "SUBMIT_COAUTHORS") return;
      context.track(
        PITCH_EVENTS.coauthorsSet,
        { count: event.coAuthors.length },
        context.trackCtx,
      );
    },
    trackReviewReached: ({ context }) =>
      context.track(PITCH_EVENTS.reviewReached, {}, context.trackCtx),
    trackSubmitting: ({ context }) =>
      context.track(PITCH_EVENTS.submitting, {}, context.trackCtx),
    trackSubmitted: ({ context }) =>
      context.track(
        PITCH_EVENTS.submitted,
        { proposal_id: context.result?.proposalId, stub: true },
        context.trackCtx,
      ),
    trackError: ({ context }) =>
      context.track(PITCH_EVENTS.error, { error: context.error }, context.trackCtx),
  },
}).createMachine({
  id: "pitchSubmitWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    meetsGate: input.meetsGate,
    votingPower: input.votingPower,
    submit: input.submit ?? simulateSubmit,
    track: input.track ?? defaultTrack,
    draft: emptyDraft(),
    errors: {},
  }),
  initial: "intro",
  states: {
    intro: {
      entry: "trackStarted",
      on: {
        PASS_GATE: {
          guard: "meetsGate",
          target: "details",
          actions: "trackGatePassed",
        },
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
            actions: ["setDetailErrors", "trackDetailsInvalid"],
          },
        ],
      },
    },
    coauthors: {
      on: {
        SUBMIT_COAUTHORS: {
          target: "review",
          actions: ["saveCoAuthors", "trackCoAuthorsSet"],
        },
        BACK: { target: "details", actions: assign({ errors: () => ({}) }) },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        CONFIRM: { target: "submitting" },
        BACK: { target: "coauthors" },
      },
    },
    submitting: {
      entry: ["trackSubmitting", assign({ error: undefined })],
      invoke: {
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({ draft: context.draft, submit: context.submit }),
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
            "trackError",
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

export type PitchMachine = typeof pitchMachine;

export function resolvePitchSnapshot(args: {
  step: PitchStateId;
  trackCtx: TrackContext;
  meetsGate: boolean;
  votingPower: number;
  submit?: SubmitFn;
  track?: TrackFn;
  draft?: PitchDraft;
}) {
  const { step, trackCtx, meetsGate, votingPower, submit, track, draft } = args;
  if (step === "intro") return undefined;
  const context: PitchContext = {
    trackCtx,
    meetsGate,
    votingPower,
    submit: submit ?? simulateSubmit,
    track: track ?? defaultTrack,
    draft: draft ?? emptyDraft(),
    errors: {},
  };
  return pitchMachine.resolveState({ value: step, context });
}
