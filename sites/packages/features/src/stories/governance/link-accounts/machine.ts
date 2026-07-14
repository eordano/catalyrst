import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import {
  failClosedVerify,
  failClosedUnlink,
  type Provider,
  type VerifyResult,
  type UnlinkResult,
} from "@data/lib/catalyst/governance/link-accounts";
import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type VerifyFn = (args: {
  provider: Provider;
  signal?: AbortSignal;
}) => Promise<VerifyResult>;

export type UnlinkFn = (args: {
  account: Provider;
  signal?: AbortSignal;
}) => Promise<UnlinkResult>;

export type LinkInput = {
  account?: Provider;
  trackCtx: TrackContext;
  verify?: VerifyFn;
  unlink?: UnlinkFn;
  track?: TrackFn;
};

export type LinkContext = {
  account: Provider;
  trackCtx: TrackContext;
  verify: VerifyFn;
  unlink: UnlinkFn;
  track: TrackFn;
  connectStep: number;
  totalSteps: number;
  result?: VerifyResult;
  unlinkResult?: UnlinkResult;
  error?: string;
};

export type LinkEvent =
  | { type: "CHOOSE"; account: Provider; totalSteps?: number }
  | { type: "NEXT_STEP" }
  | { type: "CONFIRM" }
  | { type: "BACK" }
  | { type: "RETRY" }
  | { type: "UNLINK_REQUEST"; account: Provider }
  | { type: "CONFIRM_UNLINK" }
  | { type: "CANCEL" };

export const LINK_EVENTS = {
  started: "gv_link_started",
  connectStep: "gv_link_connect_step",
  verifying: "gv_link_verifying",
  connected: "gv_link_connected",
  verifyError: "gv_link_verify_error",
  unlinked: "gv_link_unlinked",
} as const;

export const STATE_TO_SLUG = {
  choosing: "choose",
  connecting: "connect",
  verifying: "verifying",
  connected: "connected",
  error: "error",
  unlinkConfirm: "unlink",
  unlinking: "unlinking",
} as const;

export type LinkStateId = keyof typeof STATE_TO_SLUG;
export type LinkStepSlug = (typeof STATE_TO_SLUG)[LinkStateId];

export const FIRST_STEP_SLUG: LinkStepSlug = STATE_TO_SLUG.choosing;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "choosing");

export const SLUG_TO_STATE: Record<LinkStepSlug, LinkStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => LinkStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => LinkStateId = stepSlugs.toState;

export function stepsFor(account: Provider): number {
  return account === "push" ? 1 : 3;
}

export const defaultVerify: VerifyFn = ({ provider, signal }) =>
  failClosedVerify({ provider, signal });

export const defaultUnlink: UnlinkFn = ({ account, signal }) =>
  failClosedUnlink({ account, signal });

export const linkAccountsMachine = setup({
  types: {
    context: {} as LinkContext,
    events: {} as LinkEvent,
    input: {} as LinkInput,
  },
  actors: {
    runVerify: fromPromise<VerifyResult, { account: Provider; verify: VerifyFn }>(
      ({ input, signal }) => input.verify({ provider: input.account, signal }),
    ),
    runUnlink: fromPromise<UnlinkResult, { account: Provider; unlink: UnlinkFn }>(
      ({ input, signal }) => input.unlink({ account: input.account, signal }),
    ),
  },
  guards: {
    atLastStep: ({ context }) => context.connectStep >= context.totalSteps,
  },
  actions: {
    setAccount: assign({
      account: ({ context, event }) =>
        event.type === "CHOOSE" || event.type === "UNLINK_REQUEST"
          ? event.account
          : context.account,
      connectStep: 1,
      totalSteps: ({ context, event }) =>
        event.type === "CHOOSE"
          ? (event.totalSteps ?? stepsFor(event.account))
          : context.totalSteps,
    }),
    advanceStep: assign({
      connectStep: ({ context }) =>
        Math.min(context.connectStep + 1, context.totalSteps),
    }),
    trackStarted: ({ context }) =>
      context.track(LINK_EVENTS.started, { account: context.account }, context.trackCtx),
    trackConnectStep: ({ context }) =>
      context.track(
        LINK_EVENTS.connectStep,
        { account: context.account, step: context.connectStep },
        context.trackCtx,
      ),
    trackVerifying: ({ context }) =>
      context.track(LINK_EVENTS.verifying, { account: context.account }, context.trackCtx),
    trackConnected: ({ context }) =>
      context.track(
        LINK_EVENTS.connected,
        { account: context.account },
        context.trackCtx,
      ),
    trackVerifyError: ({ context }) =>
      context.track(
        LINK_EVENTS.verifyError,
        { account: context.account, error: context.error },
        context.trackCtx,
      ),
    trackUnlinked: ({ context }) =>
      context.track(
        LINK_EVENTS.unlinked,
        { account: context.account },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "linkAccounts",
  context: ({ input }) => ({
    account: input.account ?? "forum",
    trackCtx: input.trackCtx,
    verify: input.verify ?? defaultVerify,
    unlink: input.unlink ?? defaultUnlink,
    track: input.track ?? defaultTrack,
    connectStep: 1,
    totalSteps: stepsFor(input.account ?? "forum"),
  }),
  initial: "choosing",
  states: {
    choosing: {
      on: {
        CHOOSE: { target: "connecting", actions: ["setAccount", "trackStarted"] },
        UNLINK_REQUEST: { target: "unlinkConfirm", actions: "setAccount" },
      },
    },
    connecting: {
      on: {
        NEXT_STEP: { actions: ["advanceStep", "trackConnectStep"] },
        CONFIRM: { target: "verifying", guard: "atLastStep" },
        BACK: { target: "choosing" },
      },
    },
    verifying: {
      entry: [assign({ error: undefined }), "trackVerifying"],
      invoke: {
        id: "runVerify",
        src: "runVerify",
        input: ({ context }) => ({ account: context.account, verify: context.verify }),
        onDone: {
          target: "connected",
          actions: [assign({ result: ({ event }) => event.output }), "trackConnected"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "verification failed"),
          }),
        },
      },
    },
    connected: {
      type: "final",
    },
    error: {
      entry: "trackVerifyError",
      on: {
        RETRY: { target: "verifying" },
        BACK: { target: "connecting" },
      },
    },
    unlinkConfirm: {
      on: {
        CONFIRM_UNLINK: { target: "unlinking" },
        CANCEL: { target: "choosing" },
      },
    },
    unlinking: {
      invoke: {
        id: "runUnlink",
        src: "runUnlink",
        input: ({ context }) => ({ account: context.account, unlink: context.unlink }),
        onDone: {
          target: "choosing",
          actions: [
            assign({ unlinkResult: ({ event }) => event.output }),
            "trackUnlinked",
          ],
        },
        onError: {
          target: "unlinkConfirm",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "unlink failed"),
          }),
        },
      },
    },
  },
});

export type LinkAccountsMachine = typeof linkAccountsMachine;

export function resolveLinkSnapshot(args: {
  step: LinkStateId;
  account: Provider;
  trackCtx: TrackContext;
  verify?: VerifyFn;
  unlink?: UnlinkFn;
  track?: TrackFn;
}) {
  const { step, account, trackCtx, verify, unlink, track } = args;
  if (step === "choosing") return undefined;
  const total = stepsFor(account);
  const context: LinkContext = {
    account,
    trackCtx,
    verify: verify ?? defaultVerify,
    unlink: unlink ?? defaultUnlink,
    track: track ?? defaultTrack,
    connectStep: step === "connecting" ? total : 1,
    totalSteps: total,
  };
  return linkAccountsMachine.resolveState({ value: step, context });
}
