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
  | { type: "APPROVE_MANA" }
  | { type: "CONFIRM_MINT" }
  | { type: "EDIT" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const CLAIM_EVENTS = {
  started: "mk_claim_name_started",
  available: "mk_claim_name_available",
  unavailable: "mk_claim_name_unavailable",
  manaApproved: "mk_claim_name_mana_approved",
  confirmReached: "mk_claim_name_confirm_reached",
  submitted: "mk_claim_name_submitted",
  completed: "mk_claim_name_completed",
} as const;

export const STATE_TO_SLUG = {
  entering: "enter-name",
  checking: "check-availability",
  unavailable: "unavailable",
  approving: "approve-mana",
  confirming: "confirm-mint",
  submitting: "submit-tx",
  success: "success",
  error: "error",
} as const;

export type ClaimStateId = keyof typeof STATE_TO_SLUG;
export type ClaimStepSlug = (typeof STATE_TO_SLUG)[ClaimStateId];

export const FIRST_STEP_SLUG: ClaimStepSlug = STATE_TO_SLUG.entering;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "entering");

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
    trackManaApproved: ({ context }) =>
      context.track(
        CLAIM_EVENTS.manaApproved,
        { name: context.name, price_mana: "100", simulated: true },
        context.trackCtx,
      ),
    trackConfirmReached: ({ context }) =>
      context.track(
        CLAIM_EVENTS.confirmReached,
        { name: context.name, price_mana: "100" },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        CLAIM_EVENTS.submitted,
        { name: context.name, simulated: true },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        CLAIM_EVENTS.completed,
        {
          name: context.name,
          tx_hash: context.result?.txHash,
          token_id: context.result?.tokenId,
          stub: true,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "claimNameWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    taken: new Set((input.takenNames ?? []).map((n) => n.toLowerCase())),
    check: input.check ?? makeSimulateCheck(new Set((input.takenNames ?? []).map((n) => n.toLowerCase()))),
    mint: input.mint ?? simulateMint,
    track: input.track ?? defaultTrack,
    name: "",
  }),
  initial: "entering",
  states: {
    entering: {
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
            target: "approving",
            actions: "trackAvailable",
          },
          { target: "unavailable", actions: "trackUnavailable" },
        ],
        onError: { target: "unavailable", actions: "trackUnavailable" },
      },
    },
    unavailable: {
      on: {
        EDIT: { target: "entering" },
        BACK: { target: "entering" },
      },
    },
    approving: {
      on: {
        APPROVE_MANA: { target: "confirming", actions: "trackManaApproved" },
        BACK: { target: "entering" },
      },
    },
    confirming: {
      entry: [assign({ error: undefined }), "trackConfirmReached"],
      on: {
        CONFIRM_MINT: { target: "submitting", actions: "trackSubmitted" },
        BACK: { target: "entering" },
      },
    },
    submitting: {
      invoke: {
        id: "runMint",
        src: "runMint",
        input: ({ context }) => ({ name: context.name, mint: context.mint }),
        onDone: {
          target: "success",
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
    success: {
      type: "final",
    },
    error: {
      on: {
        RETRY: { target: "submitting" },
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
  if (step === "entering") return undefined;
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
