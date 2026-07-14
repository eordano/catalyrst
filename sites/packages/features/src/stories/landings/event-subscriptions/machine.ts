import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type CommitKind = "subscribe" | "unsubscribe";

export type CommitResult = { kind: CommitKind; at: number };

export type CommitFn = (args: {
  kind: CommitKind;
  enabledTypes: string[];
  signal?: AbortSignal;
}) => Promise<CommitResult>;

export type Selection = Record<string, boolean>;

export type SubscriptionInput = {
  trackCtx: TrackContext;
  selection?: Selection;
  commit?: CommitFn;
  track?: TrackFn;
};

export type SubscriptionContext = {
  trackCtx: TrackContext;
  commit: CommitFn;
  track: TrackFn;
  selection: Selection;
  lastKind: CommitKind;
  result?: CommitResult;
  error?: string;
};

export type SubscriptionEvent =
  | { type: "START" }
  | { type: "SIGN_IN" }
  | { type: "TOGGLE"; notificationType: string; enabled: boolean }
  | { type: "SUBMIT" }
  | { type: "UNSUBSCRIBE" }
  | { type: "RESUBSCRIBE" }
  | { type: "EDIT" }
  | { type: "RETRY" }
  | { type: "BACK" };

export const SUBSCRIPTION_EVENTS = {
  started: "landings_subscription_started",
  signinRequired: "landings_subscription_signin_required",
  signedIn: "landings_subscription_signed_in",
  edited: "landings_subscription_edited",
  submitting: "landings_subscription_submitting",
  subscribed: "landings_subscription_subscribed",
  unsubscribing: "landings_subscription_unsubscribing",
  unsubscribed: "landings_subscription_unsubscribed",
  error: "landings_subscription_error",
} as const;

export const STATE_TO_SLUG = {
  idle: "idle",
  signinGate: "signin-gate",
  editing: "editing",
  submitting: "submitting",
  subscribed: "subscribed",
  unsubscribing: "unsubscribing",
  unsubscribed: "unsubscribed",
  error: "error",
} as const;

export type SubscriptionStateId = keyof typeof STATE_TO_SLUG;
export type SubscriptionStepSlug = (typeof STATE_TO_SLUG)[SubscriptionStateId];

export const FIRST_STEP_SLUG: SubscriptionStepSlug = STATE_TO_SLUG.idle;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "idle");

export const SLUG_TO_STATE: Record<SubscriptionStepSlug, SubscriptionStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => SubscriptionStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => SubscriptionStateId = stepSlugs.toState;

export const simulateCommit: CommitFn = async ({ kind, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 350);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { kind, at: Date.now() };
};

export const subscriptionMachine = setup({
  types: {
    context: {} as SubscriptionContext,
    events: {} as SubscriptionEvent,
    input: {} as SubscriptionInput,
  },
  actors: {
    runCommit: fromPromise<
      CommitResult,
      { kind: CommitKind; enabledTypes: string[]; commit: CommitFn }
    >(({ input, signal }) =>
      input.commit({ kind: input.kind, enabledTypes: input.enabledTypes, signal }),
    ),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(SUBSCRIPTION_EVENTS.started, {}, context.trackCtx),
    trackSigninRequired: ({ context }) =>
      context.track(SUBSCRIPTION_EVENTS.signinRequired, {}, context.trackCtx),
    trackSignedIn: ({ context }) =>
      context.track(SUBSCRIPTION_EVENTS.signedIn, {}, context.trackCtx),
    applyToggle: assign({
      selection: ({ context, event }) =>
        event.type === "TOGGLE"
          ? { ...context.selection, [event.notificationType]: event.enabled }
          : context.selection,
    }),
    trackEdited: ({ context, event }) => {
      if (event.type !== "TOGGLE") return;
      context.track(
        SUBSCRIPTION_EVENTS.edited,
        { notification_type: event.notificationType, enabled: event.enabled },
        context.trackCtx,
      );
    },
    setSubmitKind: assign({ lastKind: () => "subscribe" as const }),
    setUnsubscribeKind: assign({ lastKind: () => "unsubscribe" as const }),
    trackSubmitting: ({ context }) =>
      context.track(
        SUBSCRIPTION_EVENTS.submitting,
        { enabled_count: enabledTypes(context.selection).length },
        context.trackCtx,
      ),
    trackUnsubscribing: ({ context }) =>
      context.track(SUBSCRIPTION_EVENTS.unsubscribing, {}, context.trackCtx),
    trackSubscribed: ({ context }) =>
      context.track(
        SUBSCRIPTION_EVENTS.subscribed,
        { enabled_count: enabledTypes(context.selection).length },
        context.trackCtx,
      ),
    trackUnsubscribed: ({ context }) =>
      context.track(SUBSCRIPTION_EVENTS.unsubscribed, {}, context.trackCtx),
    trackError: ({ context }) =>
      context.track(
        SUBSCRIPTION_EVENTS.error,
        { kind: context.lastKind, message: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "subscriptionWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    commit: input.commit ?? simulateCommit,
    track: input.track ?? defaultTrack,
    selection: input.selection ?? {},
    lastKind: "subscribe",
  }),
  initial: "idle",
  states: {
    idle: {
      on: {
        START: { target: "signinGate", actions: "trackStarted" },
      },
    },
    signinGate: {
      entry: "trackSigninRequired",
      on: {
        SIGN_IN: { target: "editing", actions: "trackSignedIn" },
      },
    },
    editing: {
      on: {
        TOGGLE: { actions: ["applyToggle", "trackEdited"] },
        SUBMIT: {
          target: "submitting",
          actions: ["setSubmitKind", "trackSubmitting"],
        },
      },
    },
    submitting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSubmit",
        src: "runCommit",
        input: ({ context }) => ({
          kind: "subscribe" as const,
          enabledTypes: enabledTypes(context.selection),
          commit: context.commit,
        }),
        onDone: {
          target: "subscribed",
          actions: [assign({ result: ({ event }) => event.output }), "trackSubscribed"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "subscribe failed"),
          }),
        },
      },
    },
    subscribed: {
      on: {
        UNSUBSCRIBE: {
          target: "unsubscribing",
          actions: ["setUnsubscribeKind", "trackUnsubscribing"],
        },
        EDIT: { target: "editing" },
      },
    },
    unsubscribing: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runUnsubscribe",
        src: "runCommit",
        input: ({ context }) => ({
          kind: "unsubscribe" as const,
          enabledTypes: enabledTypes(context.selection),
          commit: context.commit,
        }),
        onDone: {
          target: "unsubscribed",
          actions: [assign({ result: ({ event }) => event.output }), "trackUnsubscribed"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "unsubscribe failed"),
          }),
        },
      },
    },
    unsubscribed: {
      on: {
        RESUBSCRIBE: { target: "editing" },
      },
    },
    error: {
      entry: "trackError",
      on: {
        RETRY: [
          {
            guard: ({ context }) => context.lastKind === "unsubscribe",
            target: "unsubscribing",
          },
          { target: "submitting" },
        ],
        BACK: { target: "editing" },
      },
    },
  },
});

export type SubscriptionMachine = typeof subscriptionMachine;

export function enabledTypes(selection: Selection): string[] {
  return Object.entries(selection)
    .filter(([, on]) => on)
    .map(([type]) => type);
}

export function resolveSubscriptionSnapshot(args: {
  step: SubscriptionStateId;
  trackCtx: TrackContext;
  selection?: Selection;
  commit?: CommitFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, selection, commit, track } = args;
  if (step === "idle") return undefined;
  const context: SubscriptionContext = {
    trackCtx,
    commit: commit ?? simulateCommit,
    track: track ?? defaultTrack,
    selection: selection ?? {},
    lastKind:
      step === "unsubscribing" || step === "unsubscribed" ? "unsubscribe" : "subscribe",
  };
  return subscriptionMachine.resolveState({ value: step, context });
}
