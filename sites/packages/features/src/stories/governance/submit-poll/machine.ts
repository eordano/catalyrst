import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export const POLL_LIMITS = {
  title: { min: 5, max: 80 },
  description: { min: 20, max: 7000 },
  choice: { min: 1, max: 100 },
  choices: { min: 2, max: 100 },
  coAuthor: { len: 42 },
} as const;

export type PollDraft = {
  title: string;
  description: string;
  options: string[];
  coAuthors: string[];
};

export const EMPTY_DRAFT: PollDraft = {
  title: "",
  description: "",
  options: ["", ""],
  coAuthors: [],
};

export type { TrackFn };

export type SubmitResult = { proposalRef: string };

export type SubmitFn = (args: {
  draft: PollDraft;
  signal?: AbortSignal;
}) => Promise<SubmitResult>;

export type GateInput = {
  connected: boolean;
  hasVp: boolean;
};

export type SubmitPollInput = {
  trackCtx: TrackContext;
  gate: GateInput;
  draft?: PollDraft;
  submitPoll?: SubmitFn;
  track?: TrackFn;
};

export type SubmitPollContext = {
  trackCtx: TrackContext;
  gate: GateInput;
  draft: PollDraft;
  submitPoll: SubmitFn;
  track: TrackFn;
  result?: SubmitResult;
  error?: string;
};

export type SubmitPollEvent =
  | { type: "NEXT" }
  | { type: "BACK" }
  | { type: "SET_DETAILS"; title: string; description: string }
  | { type: "SET_OPTIONS"; options: string[]; coAuthors: string[] }
  | { type: "SUBMIT" }
  | { type: "RETRY" };

export const SUBMIT_POLL_EVENTS = {
  started: "gv_submit_poll_started",
  vpBlocked: "gv_submit_poll_vp_blocked",
  detailsCompleted: "gv_submit_poll_details_completed",
  optionsCompleted: "gv_submit_poll_options_completed",
  reviewReached: "gv_submit_poll_review_reached",
  submitted: "gv_submit_poll_submitted",
} as const;

export function cleanOptions(options: string[]): string[] {
  return options.map((o) => o.trim()).filter((o) => o.length > 0);
}

export function isTitleValid(title: string): boolean {
  const t = title.trim().length;
  return t >= POLL_LIMITS.title.min && t <= POLL_LIMITS.title.max;
}

export function isDescriptionValid(description: string): boolean {
  const d = description.trim().length;
  return d >= POLL_LIMITS.description.min && d <= POLL_LIMITS.description.max;
}

export function areDetailsValid(draft: PollDraft): boolean {
  return isTitleValid(draft.title) && isDescriptionValid(draft.description);
}

export function areOptionsValid(draft: PollDraft): boolean {
  const opts = cleanOptions(draft.options);
  if (opts.length < POLL_LIMITS.choices.min) return false;
  if (opts.length > POLL_LIMITS.choices.max) return false;
  if (opts.some((o) => o.length > POLL_LIMITS.choice.max)) return false;
  return draft.coAuthors.every((c) => c.trim().length === POLL_LIMITS.coAuthor.len);
}

export function canSubmit(gate: GateInput): boolean {
  return gate.connected && gate.hasVp;
}

export const STATE_TO_SLUG = {
  intro: "intro",
  details: "details",
  options: "options",
  review: "review",
  submitting: "submitting",
  success: "success",
  error: "error",
} as const;

export type SubmitPollStateId = keyof typeof STATE_TO_SLUG;
export type SubmitPollStepSlug = (typeof STATE_TO_SLUG)[SubmitPollStateId];

export const FIRST_STEP_SLUG: SubmitPollStepSlug = STATE_TO_SLUG.intro;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "intro");

export const SLUG_TO_STATE: Record<SubmitPollStepSlug, SubmitPollStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => SubmitPollStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => SubmitPollStateId = stepSlugs.toState;

export const simulateSubmit: SubmitFn = async ({ draft, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const slug = draft.title.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").slice(0, 32) || "poll";
  return { proposalRef: `stub:poll:${slug}` };
};

export const submitPollMachine = setup({
  types: {
    context: {} as SubmitPollContext,
    events: {} as SubmitPollEvent,
    input: {} as SubmitPollInput,
  },
  actors: {
    runSubmit: fromPromise<SubmitResult, { draft: PollDraft; submitPoll: SubmitFn }>(
      ({ input, signal }) => input.submitPoll({ draft: input.draft, signal }),
    ),
  },
  guards: {
    canPass: ({ context }) => canSubmit(context.gate),
    detailsValid: ({ context }) => areDetailsValid(context.draft),
    optionsValid: ({ context }) => areOptionsValid(context.draft),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(
        SUBMIT_POLL_EVENTS.started,
        { has_vp: context.gate.hasVp, connected: context.gate.connected },
        context.trackCtx,
      ),
    trackVpBlocked: ({ context }) =>
      context.track(
        SUBMIT_POLL_EVENTS.vpBlocked,
        { connected: context.gate.connected, has_vp: context.gate.hasVp },
        context.trackCtx,
      ),
    setDetails: assign({
      draft: ({ context, event }) =>
        event.type === "SET_DETAILS"
          ? { ...context.draft, title: event.title, description: event.description }
          : context.draft,
    }),
    setOptions: assign({
      draft: ({ context, event }) =>
        event.type === "SET_OPTIONS"
          ? { ...context.draft, options: event.options, coAuthors: event.coAuthors }
          : context.draft,
    }),
    trackDetailsCompleted: ({ context }) =>
      context.track(
        SUBMIT_POLL_EVENTS.detailsCompleted,
        {
          title_len: context.draft.title.trim().length,
          description_len: context.draft.description.trim().length,
        },
        context.trackCtx,
      ),
    trackOptionsCompleted: ({ context }) =>
      context.track(
        SUBMIT_POLL_EVENTS.optionsCompleted,
        {
          option_count: cleanOptions(context.draft.options).length,
          co_author_count: context.draft.coAuthors.length,
        },
        context.trackCtx,
      ),
    trackReviewReached: ({ context }) =>
      context.track(
        SUBMIT_POLL_EVENTS.reviewReached,
        { option_count: cleanOptions(context.draft.options).length },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        SUBMIT_POLL_EVENTS.submitted,
        { proposal_ref: context.result?.proposalRef, stub: true },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "submitPollWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    gate: input.gate,
    draft: input.draft ?? EMPTY_DRAFT,
    submitPoll: input.submitPoll ?? simulateSubmit,
    track: input.track ?? defaultTrack,
  }),
  initial: "intro",
  states: {
    intro: {
      on: {
        NEXT: [
          { target: "details", guard: "canPass", actions: "trackStarted" },
          { actions: "trackVpBlocked" },
        ],
      },
    },
    details: {
      on: {
        SET_DETAILS: { actions: "setDetails" },
        NEXT: {
          target: "options",
          guard: "detailsValid",
          actions: "trackDetailsCompleted",
        },
        BACK: { target: "intro" },
      },
    },
    options: {
      on: {
        SET_OPTIONS: { actions: "setOptions" },
        NEXT: {
          target: "review",
          guard: "optionsValid",
          actions: "trackOptionsCompleted",
        },
        BACK: { target: "details" },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        SUBMIT: { target: "submitting" },
        BACK: { target: "options" },
      },
    },
    submitting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({ draft: context.draft, submitPoll: context.submitPoll }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackSubmitted"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "submit failed"),
          }),
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

export type SubmitPollMachine = typeof submitPollMachine;

export function resolveSubmitPollSnapshot(args: {
  step: SubmitPollStateId;
  trackCtx: TrackContext;
  gate: GateInput;
  draft?: PollDraft;
  submitPoll?: SubmitFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, gate, draft, submitPoll, track } = args;
  if (step === "intro") return undefined;
  const context: SubmitPollContext = {
    trackCtx,
    gate,
    draft: draft ?? EMPTY_DRAFT,
    submitPoll: submitPoll ?? simulateSubmit,
    track: track ?? defaultTrack,
  };
  return submitPollMachine.resolveState({ value: step, context });
}
