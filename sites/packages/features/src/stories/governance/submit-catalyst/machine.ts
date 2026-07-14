import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import {
  failClosedCreateProposal,
  buildProposalPayload,
  type CatalystRequest,
  type CreatedProposal,
  type CreateProposalFn,
} from "@data/lib/catalyst/governance/submit-catalyst";
import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type NodeDetails = { owner: string; domain: string; alreadyACatalyst: boolean };
export type Rationale = { description: string; coAuthors: string[] };

export type CreateFn = (args: {
  request: CatalystRequest;
  details: NodeDetails;
  rationale: Rationale;
  signal?: AbortSignal;
}) => Promise<CreatedProposal>;

export type SubmitCatalystInput = {
  request: CatalystRequest;
  trackCtx: TrackContext;
  create?: CreateFn;
  track?: TrackFn;
};

export type SubmitCatalystContext = {
  request: CatalystRequest;
  trackCtx: TrackContext;
  create: CreateFn;
  track: TrackFn;
  details: NodeDetails;
  rationale: Rationale;
  result?: CreatedProposal;
  error?: string;
};

export type SubmitCatalystEvent =
  | { type: "FILL_DETAILS"; owner: string; domain: string; alreadyACatalyst?: boolean }
  | { type: "DOMAIN_INVALID" }
  | { type: "FILL_DESCRIPTION"; description: string; coAuthors?: string[] }
  | { type: "SUBMIT" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const CATALYST_EVENTS = {
  started: "gv_catalyst_started",
  detailsFilled: "gv_catalyst_details_filled",
  domainInvalid: "gv_catalyst_domain_invalid",
  descriptionFilled: "gv_catalyst_description_filled",
  reviewReached: "gv_catalyst_review_reached",
  submitting: "gv_catalyst_submitting",
  submitted: "gv_catalyst_submitted",
  submitError: "gv_catalyst_submit_error",
} as const;

export const STATE_TO_SLUG = {
  details: "details",
  description: "description",
  review: "review",
  submitting: "submitting",
  success: "success",
  error: "error",
} as const;

export type CatalystStateId = keyof typeof STATE_TO_SLUG;
export type CatalystStepSlug = (typeof STATE_TO_SLUG)[CatalystStateId];

export const FIRST_STEP_SLUG: CatalystStepSlug = STATE_TO_SLUG.details;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "details");

export const SLUG_TO_STATE: Record<CatalystStepSlug, CatalystStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => CatalystStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => CatalystStateId = stepSlugs.toState;

const EMPTY_DETAILS: NodeDetails = { owner: "", domain: "", alreadyACatalyst: false };
const EMPTY_RATIONALE: Rationale = { description: "", coAuthors: [] };

export function makeCreate(create: CreateProposalFn): CreateFn {
  return ({ request, details, rationale, signal }) =>
    create({
      payload: buildProposalPayload(request, {
        owner: details.owner,
        domain: details.domain,
        description: rationale.description,
        coAuthors: rationale.coAuthors,
      }),
      signal,
    });
}

export const defaultCreate: CreateFn = makeCreate(failClosedCreateProposal);

export const submitCatalystMachine = setup({
  types: {
    context: {} as SubmitCatalystContext,
    events: {} as SubmitCatalystEvent,
    input: {} as SubmitCatalystInput,
  },
  actors: {
    runCreate: fromPromise<
      CreatedProposal,
      { request: CatalystRequest; details: NodeDetails; rationale: Rationale; create: CreateFn }
    >(({ input, signal }) =>
      input.create({
        request: input.request,
        details: input.details,
        rationale: input.rationale,
        signal,
      }),
    ),
  },
  actions: {
    setDetails: assign({
      details: ({ event }) =>
        event.type === "FILL_DETAILS"
          ? {
              owner: event.owner,
              domain: event.domain,
              alreadyACatalyst: event.alreadyACatalyst ?? false,
            }
          : EMPTY_DETAILS,
    }),
    setRationale: assign({
      rationale: ({ event }) =>
        event.type === "FILL_DESCRIPTION"
          ? { description: event.description, coAuthors: event.coAuthors ?? [] }
          : EMPTY_RATIONALE,
    }),
    trackStarted: ({ context }) => {
      context.track(CATALYST_EVENTS.started, { request: context.request }, context.trackCtx);
      context.track(
        CATALYST_EVENTS.detailsFilled,
        { request: context.request, already_a_catalyst: context.details.alreadyACatalyst },
        context.trackCtx,
      );
    },
    trackDomainInvalid: ({ context }) =>
      context.track(
        CATALYST_EVENTS.domainInvalid,
        { request: context.request },
        context.trackCtx,
      ),
    trackDescriptionFilled: ({ context }) =>
      context.track(
        CATALYST_EVENTS.descriptionFilled,
        {
          request: context.request,
          length: context.rationale.description.length,
          coauthors: context.rationale.coAuthors.length,
        },
        context.trackCtx,
      ),
    trackReviewReached: ({ context }) =>
      context.track(
        CATALYST_EVENTS.reviewReached,
        { request: context.request },
        context.trackCtx,
      ),
    trackSubmitting: ({ context }) =>
      context.track(
        CATALYST_EVENTS.submitting,
        { request: context.request },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        CATALYST_EVENTS.submitted,
        {
          request: context.request,
          proposal_id: context.result?.id,
        },
        context.trackCtx,
      ),
    trackSubmitError: ({ context }) =>
      context.track(
        CATALYST_EVENTS.submitError,
        { request: context.request, error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "submitCatalyst",
  context: ({ input }) => ({
    request: input.request,
    trackCtx: input.trackCtx,
    create: input.create ?? defaultCreate,
    track: input.track ?? defaultTrack,
    details: EMPTY_DETAILS,
    rationale: EMPTY_RATIONALE,
  }),
  initial: "details",
  states: {
    details: {
      on: {
        FILL_DETAILS: {
          target: "description",
          actions: ["setDetails", "trackStarted"],
        },
        DOMAIN_INVALID: { actions: "trackDomainInvalid" },
      },
    },
    description: {
      on: {
        FILL_DESCRIPTION: {
          target: "review",
          actions: ["setRationale", "trackDescriptionFilled"],
        },
        BACK: { target: "details" },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        SUBMIT: { target: "submitting" },
        BACK: { target: "description" },
      },
    },
    submitting: {
      entry: [assign({ error: undefined }), "trackSubmitting"],
      invoke: {
        id: "runCreate",
        src: "runCreate",
        input: ({ context }) => ({
          request: context.request,
          details: context.details,
          rationale: context.rationale,
          create: context.create,
        }),
        onDone: {
          target: "success",
          actions: [assign({ result: ({ event }) => event.output }), "trackSubmitted"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "createProposal failed"),
          }),
        },
      },
    },
    success: {
      type: "final",
    },
    error: {
      entry: "trackSubmitError",
      on: {
        RETRY: { target: "submitting" },
        BACK: { target: "review" },
      },
    },
  },
});

export type SubmitCatalystMachine = typeof submitCatalystMachine;

export function resolveCatalystSnapshot(args: {
  step: CatalystStateId;
  request: CatalystRequest;
  trackCtx: TrackContext;
  create?: CreateFn;
  track?: TrackFn;
  details?: NodeDetails;
  rationale?: Rationale;
}) {
  const { step, request, trackCtx, create, track, details, rationale } = args;
  if (step === "details") return undefined;
  const context: SubmitCatalystContext = {
    request,
    trackCtx,
    create: create ?? defaultCreate,
    track: track ?? defaultTrack,
    details: details ?? { ...EMPTY_DETAILS },
    rationale: rationale ?? { ...EMPTY_RATIONALE },
  };
  return submitCatalystMachine.resolveState({ value: step, context });
}
