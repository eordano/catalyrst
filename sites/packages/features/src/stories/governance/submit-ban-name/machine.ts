import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  validateName,
  validateDescription,
  type FieldErrors,
} from "@data/lib/catalyst/governance/submit-ban-name";

export type { TrackFn };

export type BanNameDraft = {
  name: string;
  description: string;
  coAuthors: string[];
};

export type SubmitResult = { proposalId: string; stub: true };

export type SubmitFn = (args: {
  draft: BanNameDraft;
  signal?: AbortSignal;
}) => Promise<SubmitResult>;

export type BanNameInput = {
  trackCtx: TrackContext;
  submit?: SubmitFn;
  track?: TrackFn;
};

export type BanNameContext = {
  trackCtx: TrackContext;
  submit: SubmitFn;
  track: TrackFn;
  draft: BanNameDraft;
  errors: FieldErrors;
  result?: SubmitResult;
  error?: string;
};

export type BanNameEvent =
  | { type: "SUBMIT_NAME"; name: string }
  | { type: "SUBMIT_DESCRIPTION"; description: string; coAuthors: string[] }
  | { type: "CONFIRM" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const BAN_NAME_EVENTS = {
  started: "gv_ban_name_started",
  nameSubmitted: "gv_ban_name_name_submitted",
  nameInvalid: "gv_ban_name_name_invalid",
  descriptionSubmitted: "gv_ban_name_description_submitted",
  reviewReached: "gv_ban_name_review_reached",
  submitting: "gv_ban_name_submitting",
  submitted: "gv_ban_name_submitted",
  error: "gv_ban_name_error",
} as const;

export const STATE_TO_SLUG = {
  details: "details",
  description: "description",
  review: "review",
  submitting: "submitting",
  success: "success",
  error: "error",
} as const;

export type BanNameStateId = keyof typeof STATE_TO_SLUG;
export type BanNameStepSlug = (typeof STATE_TO_SLUG)[BanNameStateId];

export const FIRST_STEP_SLUG: BanNameStepSlug = STATE_TO_SLUG.details;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "details");

export const SLUG_TO_STATE: Record<BanNameStepSlug, BanNameStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => BanNameStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => BanNameStateId = stepSlugs.toState;

export function emptyDraft(): BanNameDraft {
  return { name: "", description: "", coAuthors: [] };
}

export const simulateSubmit: SubmitFn = async ({ signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const proposalId = `sim-ban-name-${Date.now().toString(36)}`;
  return { proposalId, stub: true };
};

export const banNameMachine = setup({
  types: {
    context: {} as BanNameContext,
    events: {} as BanNameEvent,
    input: {} as BanNameInput,
  },
  actors: {
    runSubmit: fromPromise<SubmitResult, { draft: BanNameDraft; submit: SubmitFn }>(
      ({ input, signal }) => input.submit({ draft: input.draft, signal }),
    ),
  },
  guards: {
    nameValid: ({ event }) => {
      if (event.type !== "SUBMIT_NAME") return false;
      return Object.keys(validateName(event.name)).length === 0;
    },
    descriptionValid: ({ event }) => {
      if (event.type !== "SUBMIT_DESCRIPTION") return false;
      return Object.keys(validateDescription(event.description)).length === 0;
    },
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(BAN_NAME_EVENTS.started, {}, context.trackCtx),
    saveName: assign({
      draft: ({ context, event }) =>
        event.type === "SUBMIT_NAME"
          ? { ...context.draft, name: event.name.trim() }
          : context.draft,
      errors: () => ({}),
    }),
    setNameErrors: assign({
      errors: ({ event }) =>
        event.type === "SUBMIT_NAME" ? validateName(event.name) : {},
    }),
    trackNameSubmitted: ({ context, event }) => {
      if (event.type !== "SUBMIT_NAME") return;
      context.track(
        BAN_NAME_EVENTS.nameSubmitted,
        { length: event.name.trim().length },
        context.trackCtx,
      );
    },
    trackNameInvalid: ({ context, event }) => {
      if (event.type !== "SUBMIT_NAME") return;
      context.track(
        BAN_NAME_EVENTS.nameInvalid,
        { length: event.name.trim().length },
        context.trackCtx,
      );
    },
    saveDescription: assign({
      draft: ({ context, event }) =>
        event.type === "SUBMIT_DESCRIPTION"
          ? { ...context.draft, description: event.description, coAuthors: event.coAuthors }
          : context.draft,
      errors: () => ({}),
    }),
    setDescriptionErrors: assign({
      errors: ({ event }) =>
        event.type === "SUBMIT_DESCRIPTION"
          ? validateDescription(event.description)
          : {},
    }),
    trackDescriptionSubmitted: ({ context, event }) => {
      if (event.type !== "SUBMIT_DESCRIPTION") return;
      context.track(
        BAN_NAME_EVENTS.descriptionSubmitted,
        { length: event.description.trim().length, co_authors: event.coAuthors.length },
        context.trackCtx,
      );
    },
    trackReviewReached: ({ context }) =>
      context.track(BAN_NAME_EVENTS.reviewReached, {}, context.trackCtx),
    trackSubmitting: ({ context }) =>
      context.track(BAN_NAME_EVENTS.submitting, {}, context.trackCtx),
    trackSubmitted: ({ context }) =>
      context.track(
        BAN_NAME_EVENTS.submitted,
        { proposal_id: context.result?.proposalId, stub: true },
        context.trackCtx,
      ),
    trackError: ({ context }) =>
      context.track(
        BAN_NAME_EVENTS.error,
        { error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "banNameSubmitWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    submit: input.submit ?? simulateSubmit,
    track: input.track ?? defaultTrack,
    draft: emptyDraft(),
    errors: {},
  }),
  initial: "details",
  states: {
    details: {
      entry: "trackStarted",
      on: {
        SUBMIT_NAME: [
          {
            guard: "nameValid",
            target: "description",
            actions: ["saveName", "trackNameSubmitted"],
          },
          {
            actions: ["setNameErrors", "trackNameInvalid"],
          },
        ],
      },
    },
    description: {
      on: {
        SUBMIT_DESCRIPTION: [
          {
            guard: "descriptionValid",
            target: "review",
            actions: ["saveDescription", "trackDescriptionSubmitted"],
          },
          {
            actions: "setDescriptionErrors",
          },
        ],
        BACK: { target: "details", actions: assign({ errors: () => ({}) }) },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        CONFIRM: { target: "submitting" },
        BACK: { target: "description" },
      },
    },
    submitting: {
      entry: ["trackSubmitting", assign({ error: undefined })],
      invoke: {
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({
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

export type BanNameMachine = typeof banNameMachine;

export function resolveBanNameSnapshot(args: {
  step: BanNameStateId;
  trackCtx: TrackContext;
  submit?: SubmitFn;
  track?: TrackFn;
  draft?: BanNameDraft;
}) {
  const { step, trackCtx, submit, track, draft } = args;
  if (step === "details") return undefined;
  const context: BanNameContext = {
    trackCtx,
    submit: submit ?? simulateSubmit,
    track: track ?? defaultTrack,
    draft: draft ?? emptyDraft(),
    errors: {},
  };
  return banNameMachine.resolveState({ value: step, context });
}
