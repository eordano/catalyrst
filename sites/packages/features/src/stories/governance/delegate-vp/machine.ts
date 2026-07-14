import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  failClosedDelegate,
  type DelegateReceipt,
} from "@data/lib/catalyst/governance/delegate-vp";

export type { TrackFn };

export type DelegateFn = (args: {
  space: string;
  delegate: string;
  vp: number | null;
  signal?: AbortSignal;
}) => Promise<DelegateReceipt>;

export type DelegateInput = {
  trackCtx: TrackContext;
  space: string;
  vp: number | null;
  candidateId?: string;
  candidateAddress?: string;
  candidateName?: string;
  delegate?: DelegateFn;
  track?: TrackFn;
};

export type DelegateContext = {
  trackCtx: TrackContext;
  space: string;
  vp: number | null;
  delegate: DelegateFn;
  track: TrackFn;
  candidateId?: string;
  candidateAddress?: string;
  candidateName?: string;
  receipt?: DelegateReceipt;
  error?: string;
};

export type DelegateEvent =
  | { type: "PICK_CANDIDATE"; id: string; address: string; name: string }
  | { type: "CONFIRM" }
  | { type: "SIGN" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const DELEGATE_EVENTS = {
  started: "gv_delegate_started",
  candidateViewed: "gv_delegate_candidate_viewed",
  confirmReached: "gv_delegate_confirm_reached",
  signing: "gv_delegate_signing",
  completed: "gv_delegate_completed",
} as const;

export const STATE_TO_SLUG = {
  browsing: "browse",
  candidate: "candidate",
  confirming: "confirm",
  signing: "signing",
  done: "done",
  error: "error",
} as const;

export type DelegateStateId = keyof typeof STATE_TO_SLUG;
export type DelegateStepSlug = (typeof STATE_TO_SLUG)[DelegateStateId];

export const FIRST_STEP_SLUG: DelegateStepSlug = STATE_TO_SLUG.browsing;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "browsing");

export const SLUG_TO_STATE: Record<DelegateStepSlug, DelegateStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => DelegateStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => DelegateStateId = stepSlugs.toState;

export const delegateMachine = setup({
  types: {
    context: {} as DelegateContext,
    events: {} as DelegateEvent,
    input: {} as DelegateInput,
  },
  actors: {
    runDelegate: fromPromise<
      DelegateReceipt,
      { space: string; delegate: string; vp: number | null; fn: DelegateFn }
    >(({ input, signal }) =>
      input.fn({
        space: input.space,
        delegate: input.delegate,
        vp: input.vp,
        signal,
      }),
    ),
  },
  actions: {
    setCandidate: assign({
      candidateId: ({ event }) =>
        event.type === "PICK_CANDIDATE" ? event.id : undefined,
      candidateAddress: ({ event }) =>
        event.type === "PICK_CANDIDATE" ? event.address : undefined,
      candidateName: ({ event }) =>
        event.type === "PICK_CANDIDATE" ? event.name : undefined,
    }),
    trackStartedAndViewed: ({ context, event }) => {
      if (event.type !== "PICK_CANDIDATE") return;
      context.track(
        DELEGATE_EVENTS.started,
        { candidate_id: event.id },
        context.trackCtx,
      );
      context.track(
        DELEGATE_EVENTS.candidateViewed,
        { candidate_id: event.id },
        context.trackCtx,
      );
    },
    trackConfirmReached: ({ context }) =>
      context.track(
        DELEGATE_EVENTS.confirmReached,
        { candidate_id: context.candidateId, vp: context.vp },
        context.trackCtx,
      ),
    trackSigning: ({ context }) =>
      context.track(
        DELEGATE_EVENTS.signing,
        { candidate_id: context.candidateId, vp: context.vp },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        DELEGATE_EVENTS.completed,
        {
          candidate_id: context.candidateId,
          vp: context.vp,
          tx_hash: context.receipt?.txHash,
          tx_status: context.receipt?.status,
          chain_id: context.receipt?.chainId,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "delegateWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    space: input.space,
    vp: input.vp,
    delegate: input.delegate ?? failClosedDelegate,
    track: input.track ?? defaultTrack,
    candidateId: input.candidateId,
    candidateAddress: input.candidateAddress,
    candidateName: input.candidateName,
  }),
  initial: "browsing",
  states: {
    browsing: {
      on: {
        PICK_CANDIDATE: {
          target: "candidate",
          actions: ["setCandidate", "trackStartedAndViewed"],
        },
      },
    },
    candidate: {
      on: {
        CONFIRM: { target: "confirming" },
        BACK: { target: "browsing" },
      },
    },
    confirming: {
      entry: "trackConfirmReached",
      on: {
        SIGN: { target: "signing", actions: "trackSigning" },
        BACK: { target: "candidate" },
      },
    },
    signing: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runDelegate",
        src: "runDelegate",
        input: ({ context }) => ({
          space: context.space,
          delegate: context.candidateAddress ?? "",
          vp: context.vp,
          fn: context.delegate,
        }),
        onDone: {
          target: "done",
          actions: [
            assign({ receipt: ({ event }) => event.output }),
            "trackCompleted",
          ],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "delegation failed"),
          }),
        },
      },
    },
    done: {
      type: "final",
    },
    error: {
      on: {
        RETRY: { target: "signing" },
        BACK: { target: "confirming" },
      },
    },
  },
});

export type DelegateMachine = typeof delegateMachine;

export function resolveDelegateSnapshot(args: {
  step: DelegateStateId;
  trackCtx: TrackContext;
  space: string;
  vp: number | null;
  delegate?: DelegateFn;
  track?: TrackFn;
  candidate?: { id: string; address: string; name: string };
}) {
  const { step, trackCtx, space, vp, delegate, track, candidate } = args;
  if (step === "browsing") return undefined;
  const context: DelegateContext = {
    trackCtx,
    space,
    vp,
    delegate: delegate ?? failClosedDelegate,
    track: track ?? defaultTrack,
    candidateId: candidate?.id,
    candidateAddress: candidate?.address,
    candidateName: candidate?.name,
  };
  return delegateMachine.resolveState({ value: step, context });
}
