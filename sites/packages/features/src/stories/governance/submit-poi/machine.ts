import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  validateCoordinates,
  validateDescription,
  type FieldErrors,
  type PoiRequest,
} from "@data/lib/catalyst/governance/submit-poi";

export type { TrackFn };

export type PoiDraft = {
  x: string;
  y: string;
  description: string;
  coAuthors: string[];
};

export type SubmitResult = { proposalId: string; stub: true };

export type SubmitFn = (args: {
  request: PoiRequest;
  draft: PoiDraft;
  signal?: AbortSignal;
}) => Promise<SubmitResult>;

export type PoiInput = {
  trackCtx: TrackContext;
  request: PoiRequest;
  submit?: SubmitFn;
  track?: TrackFn;
};

export type PoiContext = {
  trackCtx: TrackContext;
  request: PoiRequest;
  submit: SubmitFn;
  track: TrackFn;
  draft: PoiDraft;
  errors: FieldErrors;
  result?: SubmitResult;
  error?: string;
};

export type PoiEvent =
  | { type: "SUBMIT_COORDINATES"; x: string; y: string }
  | { type: "SUBMIT_DESCRIPTION"; description: string; coAuthors: string[] }
  | { type: "CONFIRM" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const POI_EVENTS = {
  started: "gv_poi_started",
  coordinatesSubmitted: "gv_poi_coordinates_submitted",
  coordinatesInvalid: "gv_poi_coordinates_invalid",
  descriptionSubmitted: "gv_poi_description_submitted",
  reviewReached: "gv_poi_review_reached",
  submitting: "gv_poi_submitting",
  submitted: "gv_poi_submitted",
  error: "gv_poi_error",
} as const;

export const STATE_TO_SLUG = {
  coordinates: "coordinates",
  description: "description",
  review: "review",
  submitting: "submitting",
  success: "success",
  error: "error",
} as const;

export type PoiStateId = keyof typeof STATE_TO_SLUG;
export type PoiStepSlug = (typeof STATE_TO_SLUG)[PoiStateId];

export const FIRST_STEP_SLUG: PoiStepSlug = STATE_TO_SLUG.coordinates;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "coordinates");

export const SLUG_TO_STATE: Record<PoiStepSlug, PoiStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => PoiStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => PoiStateId = stepSlugs.toState;

export function emptyDraft(): PoiDraft {
  return { x: "", y: "", description: "", coAuthors: [] };
}

export const simulateSubmit: SubmitFn = async ({ signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const proposalId = `sim-poi-${Date.now().toString(36)}`;
  return { proposalId, stub: true };
};

export const poiMachine = setup({
  types: {
    context: {} as PoiContext,
    events: {} as PoiEvent,
    input: {} as PoiInput,
  },
  actors: {
    runSubmit: fromPromise<
      SubmitResult,
      { request: PoiRequest; draft: PoiDraft; submit: SubmitFn }
    >(({ input, signal }) =>
      input.submit({ request: input.request, draft: input.draft, signal }),
    ),
  },
  guards: {
    coordinatesValid: ({ event }) => {
      if (event.type !== "SUBMIT_COORDINATES") return false;
      return Object.keys(validateCoordinates(event.x, event.y)).length === 0;
    },
    descriptionValid: ({ event }) => {
      if (event.type !== "SUBMIT_DESCRIPTION") return false;
      return Object.keys(validateDescription(event.description)).length === 0;
    },
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(POI_EVENTS.started, { request: context.request }, context.trackCtx),
    saveCoordinates: assign({
      draft: ({ context, event }) =>
        event.type === "SUBMIT_COORDINATES"
          ? { ...context.draft, x: event.x, y: event.y }
          : context.draft,
      errors: () => ({}),
    }),
    setCoordinateErrors: assign({
      errors: ({ event }) =>
        event.type === "SUBMIT_COORDINATES"
          ? validateCoordinates(event.x, event.y)
          : {},
    }),
    trackCoordinatesSubmitted: ({ context, event }) => {
      if (event.type !== "SUBMIT_COORDINATES") return;
      context.track(
        POI_EVENTS.coordinatesSubmitted,
        { x: event.x, y: event.y, request: context.request },
        context.trackCtx,
      );
    },
    trackCoordinatesInvalid: ({ context, event }) => {
      if (event.type !== "SUBMIT_COORDINATES") return;
      context.track(
        POI_EVENTS.coordinatesInvalid,
        { x: event.x, y: event.y, request: context.request },
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
        POI_EVENTS.descriptionSubmitted,
        { length: event.description.trim().length, co_authors: event.coAuthors.length },
        context.trackCtx,
      );
    },
    trackReviewReached: ({ context }) =>
      context.track(POI_EVENTS.reviewReached, { request: context.request }, context.trackCtx),
    trackSubmitting: ({ context }) =>
      context.track(POI_EVENTS.submitting, { request: context.request }, context.trackCtx),
    trackSubmitted: ({ context }) =>
      context.track(
        POI_EVENTS.submitted,
        {
          request: context.request,
          proposal_id: context.result?.proposalId,
          stub: true,
        },
        context.trackCtx,
      ),
    trackError: ({ context }) =>
      context.track(
        POI_EVENTS.error,
        { request: context.request, error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "poiSubmitWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    request: input.request,
    submit: input.submit ?? simulateSubmit,
    track: input.track ?? defaultTrack,
    draft: emptyDraft(),
    errors: {},
  }),
  initial: "coordinates",
  states: {
    coordinates: {
      entry: "trackStarted",
      on: {
        SUBMIT_COORDINATES: [
          {
            guard: "coordinatesValid",
            target: "description",
            actions: ["saveCoordinates", "trackCoordinatesSubmitted"],
          },
          {
            actions: ["setCoordinateErrors", "trackCoordinatesInvalid"],
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
        BACK: { target: "coordinates", actions: assign({ errors: () => ({}) }) },
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

export type PoiMachine = typeof poiMachine;

export function resolvePoiSnapshot(args: {
  step: PoiStateId;
  trackCtx: TrackContext;
  request: PoiRequest;
  submit?: SubmitFn;
  track?: TrackFn;
  draft?: PoiDraft;
}) {
  const { step, trackCtx, request, submit, track, draft } = args;
  if (step === "coordinates") return undefined;
  const context: PoiContext = {
    trackCtx,
    request,
    submit: submit ?? simulateSubmit,
    track: track ?? defaultTrack,
    draft: draft ?? emptyDraft(),
    errors: {},
  };
  return poiMachine.resolveState({ value: step, context });
}
