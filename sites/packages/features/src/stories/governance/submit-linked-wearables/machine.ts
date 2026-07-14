import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import {
  failClosedCreateProposal,
  buildProposalPayload,
  validateIdentity,
  validateCollection,
  validateTechnical,
  type CreatedProposal,
  type IdentityInput,
  type CollectionInput,
  type TechnicalInput,
  type FieldErrors,
  type LinkedWearablesDraft,
  type CreateProposalFn,
} from "@data/lib/catalyst/governance/submit-linked-wearables";
import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type CreateFn = (args: {
  draft: LinkedWearablesDraft;
  signal?: AbortSignal;
}) => Promise<CreatedProposal>;

export type SubmitLwInput = {
  trackCtx: TrackContext;
  create?: CreateFn;
  track?: TrackFn;
};

export type SubmitLwContext = {
  trackCtx: TrackContext;
  create: CreateFn;
  track: TrackFn;
  identity: IdentityInput;
  collection: CollectionInput;
  technical: TechnicalInput;
  coAuthors: string[];
  errors: FieldErrors;
  result?: CreatedProposal;
  error?: string;
};

export type SubmitLwEvent =
  | { type: "FILL_IDENTITY"; identity: IdentityInput }
  | { type: "FILL_COLLECTION"; collection: CollectionInput }
  | { type: "FILL_TECHNICAL"; technical: TechnicalInput; coAuthors?: string[] }
  | { type: "SUBMIT" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const LW_EVENTS = {
  started: "gv_lw_started",
  identityFilled: "gv_lw_identity_filled",
  collectionFilled: "gv_lw_collection_filled",
  technicalFilled: "gv_lw_technical_filled",
  validationError: "gv_lw_validation_error",
  reviewReached: "gv_lw_review_reached",
  submitting: "gv_lw_submitting",
  submitted: "gv_lw_submitted",
  submitError: "gv_lw_submit_error",
} as const;

export const STATE_TO_SLUG = {
  identity: "identity",
  collection: "collection",
  technical: "technical",
  review: "review",
  submitting: "submitting",
  success: "success",
  error: "error",
} as const;

export type LwStateId = keyof typeof STATE_TO_SLUG;
export type LwStepSlug = (typeof STATE_TO_SLUG)[LwStateId];

export const FIRST_STEP_SLUG: LwStepSlug = STATE_TO_SLUG.identity;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "identity");

export const SLUG_TO_STATE: Record<LwStepSlug, LwStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => LwStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => LwStateId = stepSlugs.toState;

export const EMPTY_IDENTITY: IdentityInput = { name: "", marketplaceLink: "", links: [""] };
export const EMPTY_COLLECTION: CollectionInput = {
  imagePreviews: [""],
  nftCollections: "",
  items: "1",
};
export const EMPTY_TECHNICAL: TechnicalInput = {
  smartContracts: [""],
  managers: [""],
  programmaticallyGenerated: false,
  method: "",
};

const draftFrom = (ctx: SubmitLwContext): LinkedWearablesDraft => ({
  identity: ctx.identity,
  collection: ctx.collection,
  technical: ctx.technical,
  coAuthors: ctx.coAuthors,
});

export function makeCreate(create: CreateProposalFn): CreateFn {
  return ({ draft, signal }) =>
    create({ payload: buildProposalPayload(draft), signal });
}

export const defaultCreate: CreateFn = makeCreate(failClosedCreateProposal);

export const submitLwMachine = setup({
  types: {
    context: {} as SubmitLwContext,
    events: {} as SubmitLwEvent,
    input: {} as SubmitLwInput,
  },
  actors: {
    runCreate: fromPromise<CreatedProposal, { draft: LinkedWearablesDraft; create: CreateFn }>(
      ({ input, signal }) => input.create({ draft: input.draft, signal }),
    ),
  },
  guards: {
    identityValid: ({ event }) =>
      event.type === "FILL_IDENTITY" &&
      Object.keys(validateIdentity(event.identity)).length === 0,
    collectionValid: ({ event }) =>
      event.type === "FILL_COLLECTION" &&
      Object.keys(validateCollection(event.collection)).length === 0,
    technicalValid: ({ event }) =>
      event.type === "FILL_TECHNICAL" &&
      Object.keys(validateTechnical(event.technical)).length === 0,
  },
  actions: {
    setIdentity: assign({
      identity: ({ event }) =>
        event.type === "FILL_IDENTITY" ? event.identity : EMPTY_IDENTITY,
      errors: {},
    }),
    setCollection: assign({
      collection: ({ event }) =>
        event.type === "FILL_COLLECTION" ? event.collection : EMPTY_COLLECTION,
      errors: {},
    }),
    setTechnical: assign({
      technical: ({ event }) =>
        event.type === "FILL_TECHNICAL" ? event.technical : EMPTY_TECHNICAL,
      coAuthors: ({ context, event }) =>
        event.type === "FILL_TECHNICAL" ? event.coAuthors ?? [] : context.coAuthors,
      errors: {},
    }),
    setIdentityErrors: assign({
      errors: ({ event }) =>
        event.type === "FILL_IDENTITY" ? validateIdentity(event.identity) : {},
    }),
    setCollectionErrors: assign({
      errors: ({ event }) =>
        event.type === "FILL_COLLECTION" ? validateCollection(event.collection) : {},
    }),
    setTechnicalErrors: assign({
      errors: ({ event }) =>
        event.type === "FILL_TECHNICAL" ? validateTechnical(event.technical) : {},
    }),
    trackStarted: ({ context }) => {
      context.track(LW_EVENTS.started, {}, context.trackCtx);
      context.track(
        LW_EVENTS.identityFilled,
        { links: context.identity.links.filter((l) => l.trim()).length },
        context.trackCtx,
      );
    },
    trackCollectionFilled: ({ context }) =>
      context.track(
        LW_EVENTS.collectionFilled,
        {
          images: context.collection.imagePreviews.filter((i) => i.trim()).length,
          items: context.collection.items,
        },
        context.trackCtx,
      ),
    trackTechnicalFilled: ({ context }) =>
      context.track(
        LW_EVENTS.technicalFilled,
        {
          contracts: context.technical.smartContracts.filter((c) => c.trim()).length,
          managers: context.technical.managers.filter((m) => m.trim()).length,
          programmatic: context.technical.programmaticallyGenerated,
        },
        context.trackCtx,
      ),
    trackValidationError: ({ context }, params: { step: string }) =>
      context.track(
        LW_EVENTS.validationError,
        { step: params.step, fields: Object.keys(context.errors) },
        context.trackCtx,
      ),
    trackReviewReached: ({ context }) =>
      context.track(LW_EVENTS.reviewReached, {}, context.trackCtx),
    trackSubmitting: ({ context }) =>
      context.track(LW_EVENTS.submitting, {}, context.trackCtx),
    trackSubmitted: ({ context }) =>
      context.track(
        LW_EVENTS.submitted,
        { proposal_id: context.result?.id },
        context.trackCtx,
      ),
    trackSubmitError: ({ context }) =>
      context.track(LW_EVENTS.submitError, { error: context.error }, context.trackCtx),
  },
}).createMachine({
  id: "submitLinkedWearables",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    create: input.create ?? defaultCreate,
    track: input.track ?? defaultTrack,
    identity: { ...EMPTY_IDENTITY },
    collection: { ...EMPTY_COLLECTION },
    technical: { ...EMPTY_TECHNICAL },
    coAuthors: [],
    errors: {},
  }),
  initial: "identity",
  states: {
    identity: {
      on: {
        FILL_IDENTITY: [
          {
            guard: "identityValid",
            target: "collection",
            actions: ["setIdentity", "trackStarted"],
          },
          {
            actions: [
              "setIdentityErrors",
              { type: "trackValidationError", params: { step: "identity" } },
            ],
          },
        ],
      },
    },
    collection: {
      on: {
        FILL_COLLECTION: [
          {
            guard: "collectionValid",
            target: "technical",
            actions: ["setCollection", "trackCollectionFilled"],
          },
          {
            actions: [
              "setCollectionErrors",
              { type: "trackValidationError", params: { step: "collection" } },
            ],
          },
        ],
        BACK: { target: "identity", actions: assign({ errors: {} }) },
      },
    },
    technical: {
      on: {
        FILL_TECHNICAL: [
          {
            guard: "technicalValid",
            target: "review",
            actions: ["setTechnical", "trackTechnicalFilled"],
          },
          {
            actions: [
              "setTechnicalErrors",
              { type: "trackValidationError", params: { step: "technical" } },
            ],
          },
        ],
        BACK: { target: "collection", actions: assign({ errors: {} }) },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        SUBMIT: { target: "submitting" },
        BACK: { target: "technical" },
      },
    },
    submitting: {
      entry: [assign({ error: undefined }), "trackSubmitting"],
      invoke: {
        id: "runCreate",
        src: "runCreate",
        input: ({ context }) => ({ draft: draftFrom(context), create: context.create }),
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

export type SubmitLwMachine = typeof submitLwMachine;

export function resolveLwSnapshot(args: {
  step: LwStateId;
  trackCtx: TrackContext;
  create?: CreateFn;
  track?: TrackFn;
  identity?: IdentityInput;
  collection?: CollectionInput;
  technical?: TechnicalInput;
  coAuthors?: string[];
}) {
  const { step, trackCtx, create, track, identity, collection, technical, coAuthors } = args;
  if (step === "identity") return undefined;
  const context: SubmitLwContext = {
    trackCtx,
    create: create ?? defaultCreate,
    track: track ?? defaultTrack,
    identity: identity ?? { ...EMPTY_IDENTITY },
    collection: collection ?? { ...EMPTY_COLLECTION },
    technical: technical ?? { ...EMPTY_TECHNICAL },
    coAuthors: coAuthors ?? [],
    errors: {},
  };
  return submitLwMachine.resolveState({ value: step, context });
}
