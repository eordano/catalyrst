import { delay, hashHex, makeStepSlugs } from "@core/lib/stories/index";
import { toErrorMessage } from "@core/lib/errors";
import { assign, fromPromise, setup } from "xstate";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type AvailabilityResult = { available: boolean };

export type MintResult = { txHash: string; tokenId: string };

export type CheckAvailabilityFn = (args: {
  name: string;
  signal?: AbortSignal;
}) => Promise<AvailabilityResult>;

export type MintFn = (args: {
  name: string;
  signal?: AbortSignal;
}) => Promise<MintResult>;

export type ClaimInput = {
  trackCtx: TrackContext;
  takenNames?: string[];
  check?: CheckAvailabilityFn;
  mint?: MintFn;
  track?: TrackFn;
};

export type ClaimContext = {
  trackCtx: TrackContext;
  taken: Set<string>;
  check: CheckAvailabilityFn;
  mint: MintFn;
  track: TrackFn;
  name: string;
  result?: MintResult;
  error?: string;
};

export type ClaimEvent =
  | { type: "SUBMIT_NAME"; name: string }
  | { type: "CONFIRM_MINT" }
  | { type: "EDIT" }
  | { type: "BACK" }
  | { type: "RETRY" }
  | { type: "RETURN" };

export const CLAIM_EVENTS = {
  started: "ch_claim_name_started",
  available: "ch_claim_name_available",
  unavailable: "ch_claim_name_unavailable",
  reviewReached: "ch_claim_name_review_reached",
  mintSubmitted: "ch_claim_name_mint_submitted",
  completed: "ch_claim_name_completed",
  returned: "ch_claim_name_returned_to_publish",
} as const;

export const STATE_TO_SLUG = {
  naming: "name",
  checking: "availability",
  unavailable: "unavailable",
  reviewing: "review",
  minting: "mint",
  done: "done",
  error: "error",
} as const;

export type ClaimStateId = keyof typeof STATE_TO_SLUG;
export type ClaimStepSlug = (typeof STATE_TO_SLUG)[ClaimStateId];

export const FIRST_STEP_SLUG: ClaimStepSlug = STATE_TO_SLUG.naming;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "naming");

export const SLUG_TO_STATE: Record<ClaimStepSlug, ClaimStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => ClaimStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => ClaimStateId = stepSlugs.toState;

export function makeSimulateCheck(taken: Set<string>): CheckAvailabilityFn {
  return async ({ name, signal }) => {
    await delay(350, signal);
    return { available: !taken.has(name.trim().toLowerCase()) };
  };
}

export const simulateMint: MintFn = async ({ name, signal }) => {
  await delay(450, signal);
  const seed = `${name}-${Date.now()}`;
  return {
    txHash: `0x${hashHex(seed)}`,
    tokenId: BigInt(`0x${hashHex(name)}`).toString(),
  };
};

export const claimNameMachine = setup({
  types: {
    context: {} as ClaimContext,
    events: {} as ClaimEvent,
    input: {} as ClaimInput,
  },
  actors: {
    runCheck: fromPromise<
      AvailabilityResult,
      { name: string; check: CheckAvailabilityFn }
    >(({ input, signal }) => input.check({ name: input.name, signal })),
    runMint: fromPromise<MintResult, { name: string; mint: MintFn }>(
      ({ input, signal }) => input.mint({ name: input.name, signal }),
    ),
  },
  actions: {
    setName: assign({
      name: ({ event }) => (event.type === "SUBMIT_NAME" ? event.name.trim() : ""),
    }),
    trackStarted: ({ context, event }) => {
      if (event.type !== "SUBMIT_NAME") return;
      context.track(
        CLAIM_EVENTS.started,
        { name: event.name.trim() },
        context.trackCtx,
      );
    },
    trackAvailable: ({ context }) =>
      context.track(CLAIM_EVENTS.available, { name: context.name }, context.trackCtx),
    trackUnavailable: ({ context }) =>
      context.track(CLAIM_EVENTS.unavailable, { name: context.name }, context.trackCtx),
    trackReviewReached: ({ context }) =>
      context.track(
        CLAIM_EVENTS.reviewReached,
        { name: context.name, price_mana: "100" },
        context.trackCtx,
      ),
    trackMintSubmitted: ({ context }) =>
      context.track(
        CLAIM_EVENTS.mintSubmitted,
        { name: context.name, simulated: true },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        CLAIM_EVENTS.completed,
        {
          name: context.name,
          world_name: `${context.name.toLowerCase()}.dcl.eth`,
          tx_hash: context.result?.txHash,
          token_id: context.result?.tokenId,
          stub: true,
        },
        context.trackCtx,
      ),
    trackReturned: ({ context }) =>
      context.track(
        CLAIM_EVENTS.returned,
        { name: context.name, world_name: `${context.name.toLowerCase()}.dcl.eth` },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "creatorHubClaimName",
  context: ({ input }) => {
    const taken = new Set((input.takenNames ?? []).map((n) => n.toLowerCase()));
    return {
      trackCtx: input.trackCtx,
      taken,
      check: input.check ?? makeSimulateCheck(taken),
      mint: input.mint ?? simulateMint,
      track: input.track ?? defaultTrack,
      name: "",
    };
  },
  initial: "naming",
  states: {
    naming: {
      on: {
        SUBMIT_NAME: {
          target: "checking",
          actions: ["setName", "trackStarted"],
        },
      },
    },
    checking: {
      invoke: {
        id: "runCheck",
        src: "runCheck",
        input: ({ context }) => ({ name: context.name, check: context.check }),
        onDone: [
          {
            guard: ({ event }) => event.output.available,
            target: "reviewing",
            actions: "trackAvailable",
          },
          { target: "unavailable", actions: "trackUnavailable" },
        ],
        onError: { target: "unavailable", actions: "trackUnavailable" },
      },
    },
    unavailable: {
      on: {
        EDIT: { target: "naming" },
        BACK: { target: "naming" },
      },
    },
    reviewing: {
      entry: [assign({ error: undefined }), "trackReviewReached"],
      on: {
        CONFIRM_MINT: { target: "minting", actions: "trackMintSubmitted" },
        BACK: { target: "naming" },
      },
    },
    minting: {
      invoke: {
        id: "runMint",
        src: "runMint",
        input: ({ context }) => ({ name: context.name, mint: context.mint }),
        onDone: {
          target: "done",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "mint failed"),
          }),
        },
      },
    },
    done: {
      on: {
        RETURN: { target: "returned", actions: "trackReturned" },
      },
    },
    returned: {
      type: "final",
    },
    error: {
      on: {
        RETRY: { target: "minting" },
      },
    },
  },
});

export type ClaimNameMachine = typeof claimNameMachine;

export function resolveClaimSnapshot(args: {
  step: ClaimStateId;
  trackCtx: TrackContext;
  takenNames?: string[];
  check?: CheckAvailabilityFn;
  mint?: MintFn;
  track?: TrackFn;
  name?: string;
}) {
  const { step, trackCtx, takenNames, check, mint, track, name = "myWorld" } = args;
  if (step === "naming") return undefined;
  const taken = new Set((takenNames ?? []).map((n) => n.toLowerCase()));
  const context: ClaimContext = {
    trackCtx,
    taken,
    check: check ?? makeSimulateCheck(taken),
    mint: mint ?? simulateMint,
    track: track ?? defaultTrack,
    name,
  };
  return claimNameMachine.resolveState({ value: step, context });
}
