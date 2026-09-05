import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  simulateModerate,
  type ModerationAction,
  type PatchResult,
  type SimulatedModeration,
} from "@data/lib/catalyst/admin/whatson-admin";
import { controlStatus } from "@data/lib/catalyst/admin/control-availability";

export type { TrackFn };

export type ModerateFn = (args: {
  eventId: string;
  action: ModerationAction;
  rejectReasons?: string[];
  rejectNote?: string;
  signal?: AbortSignal;
}) => Promise<PatchResult | SimulatedModeration>;

export type ModerateInput = {
  trackCtx: TrackContext;
  moderate?: ModerateFn;
  track?: TrackFn;
};

export type ModerateContext = {
  trackCtx: TrackContext;
  moderate: ModerateFn;
  track: TrackFn;
  eventId?: string;
  action?: ModerationAction;
  rejectReasons?: string[];
  rejectNote?: string;
  result?: PatchResult | SimulatedModeration;
  error?: string;
};

export type ModerateEvent =
  | { type: "SIGN_IN" }
  | { type: "OPEN"; eventId: string }
  | { type: "CLOSE" }
  | { type: "DECIDE"; action: ModerationAction; rejectReasons?: string[]; rejectNote?: string }
  | { type: "CANCEL" }
  | { type: "CONFIRM" }
  | { type: "RETRY" }
  | { type: "CONTINUE" };

export const MODERATE_EVENTS = {
  gateViewed: "lp_whatson_admin_gate_viewed",
  authenticated: "lp_whatson_admin_authenticated",
  queueViewed: "lp_whatson_admin_queue_viewed",
  eventOpened: "lp_whatson_admin_event_opened",
  decisionMade: "lp_whatson_admin_decision_made",
  confirmed: "lp_whatson_admin_moderation_confirmed",
  failed: "lp_whatson_admin_moderation_failed",
  moderated: "lp_whatson_admin_moderated",
} as const;

export const STATE_TO_SLUG = {
  authGate: "auth-gate",
  queue: "queue",
  reviewEvent: "review-event",
  decision: "decision",
  submitting: "submitting",
  moderated: "moderated",
} as const;

export type ModerateStateId = keyof typeof STATE_TO_SLUG;
export type ModerateStepSlug = (typeof STATE_TO_SLUG)[ModerateStateId];

export const FIRST_STEP_SLUG: ModerateStepSlug = STATE_TO_SLUG.authGate;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "authGate");

export const SLUG_TO_STATE: Record<ModerateStepSlug, ModerateStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => ModerateStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => ModerateStateId = stepSlugs.toState;

/**
 * Layout / story data only. Never the default -- see `failClosedModerateAction`.
 */
// Typed by inference (SimulatedModeration) so tests can assert its shape;
// still assignable to ModerateFn, whose result is the wire-or-simulated union.
export const simulateModerateAction = ({ eventId, action, rejectReasons, rejectNote, signal }: Parameters<ModerateFn>[0]) =>
  simulateModerate({ eventId, action, rejectReasons, rejectNote }, { signal });

/**
 * The default moderation actor: it fails closed and says why.
 *
 * `simulateModerateAction` must not be the default: it resolves successfully
 * for a decision that never leaves the browser, so the wizard would reach its
 * "event approved" screen while catalyrst-events has heard nothing -- the same
 * class of failure as a privileged button that silently 403s, except worse,
 * because it reports success.
 *
 * The real gate is server-side and fails closed on its own:
 *   catalyrst-events/src/handlers/events.rs:657  `patch_event`
 *     -> catalyrst-events/src/admin.rs:34-44 `authorize_admin`
 *        token `None`    -> 403 "Admin operations are disabled"
 *        bearer mismatch -> 403 "You are not authorized to access this resource"
 * The credential is `CATALYRST_EVENTS_ADMIN_TOKEN`
 * (catalyrst-events/src/config.rs:27), which is set in no env file here. A
 * bearer-holding `.server.ts` module plus a route action is what would wire
 * this; the build gate classifies both as FIX-FIRST and neither is done.
 *
 * Until then this rejects with the same unavailable reason the rest of the
 * admin surface reports, so the wizard shows an error instead of a success.
 */
export const failClosedModerateAction: ModerateFn = async () => {
  const status = controlStatus("events.moderation.decide");
  throw new Error(
    status.ok ? "Event moderation is not wired." : status.message,
  );
};

export const moderateMachine = setup({
  types: {
    context: {} as ModerateContext,
    events: {} as ModerateEvent,
    input: {} as ModerateInput,
  },
  actors: {
    runModerate: fromPromise<
      PatchResult | SimulatedModeration,
      {
        eventId: string;
        action: ModerationAction;
        rejectReasons?: string[];
        rejectNote?: string;
        moderate: ModerateFn;
      }
    >(({ input, signal }) =>
      input.moderate({
        eventId: input.eventId,
        action: input.action,
        rejectReasons: input.rejectReasons,
        rejectNote: input.rejectNote,
        signal,
      }),
    ),
  },
  actions: {
    trackGateViewed: ({ context }) =>
      context.track(MODERATE_EVENTS.gateViewed, {}, context.trackCtx),
    trackAuthenticated: ({ context }) =>
      context.track(MODERATE_EVENTS.authenticated, { simulated_bearer: true }, context.trackCtx),
    trackQueueViewed: ({ context }) =>
      context.track(MODERATE_EVENTS.queueViewed, {}, context.trackCtx),
    setEvent: assign({
      eventId: ({ event }) => (event.type === "OPEN" ? event.eventId : undefined),
      action: undefined,
      result: undefined,
      error: undefined,
    }),
    trackEventOpened: ({ context, event }) => {
      if (event.type !== "OPEN") return;
      context.track(MODERATE_EVENTS.eventOpened, { event_id: event.eventId }, context.trackCtx);
    },
    setDecision: assign({
      action: ({ event }) => (event.type === "DECIDE" ? event.action : undefined),
      rejectReasons: ({ event }) =>
        event.type === "DECIDE" ? event.rejectReasons : undefined,
      rejectNote: ({ event }) => (event.type === "DECIDE" ? event.rejectNote : undefined),
    }),
    trackDecisionMade: ({ context, event }) => {
      if (event.type !== "DECIDE") return;
      context.track(
        MODERATE_EVENTS.decisionMade,
        { event_id: context.eventId, action: event.action },
        context.trackCtx,
      );
    },
    trackConfirmed: ({ context }) =>
      context.track(
        MODERATE_EVENTS.confirmed,
        { event_id: context.eventId, action: context.action },
        context.trackCtx,
      ),
    trackFailed: ({ context }) =>
      context.track(
        MODERATE_EVENTS.failed,
        { event_id: context.eventId, action: context.action, error: context.error },
        context.trackCtx,
      ),
    trackModerated: ({ context }) =>
      context.track(
        MODERATE_EVENTS.moderated,
        { event_id: context.eventId, action: context.action, stub: true },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "whatsonAdminModerate",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    moderate: input.moderate ?? failClosedModerateAction,
    track: input.track ?? defaultTrack,
  }),
  initial: "authGate",
  states: {
    authGate: {
      entry: "trackGateViewed",
      on: {
        SIGN_IN: { target: "queue", actions: "trackAuthenticated" },
      },
    },
    queue: {
      entry: "trackQueueViewed",
      on: {
        OPEN: { target: "reviewEvent", actions: ["setEvent", "trackEventOpened"] },
      },
    },
    reviewEvent: {
      on: {
        DECIDE: { target: "decision", actions: ["setDecision", "trackDecisionMade"] },
        CLOSE: { target: "queue" },
      },
    },
    decision: {
      on: {
        CONFIRM: {
          target: "submitting",
          actions: [assign({ error: undefined }), "trackConfirmed"],
        },
        DECIDE: { actions: ["setDecision", "trackDecisionMade"] },
        CANCEL: { target: "reviewEvent", actions: assign({ error: undefined }) },
      },
    },
    submitting: {
      invoke: {
        id: "runModerate",
        src: "runModerate",
        input: ({ context }) => ({
          eventId: context.eventId ?? "",
          action: context.action ?? "approve",
          rejectReasons: context.rejectReasons,
          rejectNote: context.rejectNote,
          moderate: context.moderate,
        }),
        onDone: {
          target: "moderated",
          actions: [assign({ result: ({ event }) => event.output }), "trackModerated"],
        },
        onError: {
          target: "decision",
          actions: [
            assign({
              error: ({ event }) =>
                toErrorMessage(event.error, "moderation failed"),
            }),
            "trackFailed",
          ],
        },
      },
    },
    moderated: {
      on: {
        CONTINUE: {
          target: "queue",
          actions: assign({
            eventId: undefined,
            action: undefined,
            result: undefined,
            error: undefined,
            rejectReasons: undefined,
            rejectNote: undefined,
          }),
        },
      },
    },
  },
});

export type ModerateMachine = typeof moderateMachine;

export function resolveModerateSnapshot(args: {
  step: ModerateStateId;
  trackCtx: TrackContext;
  moderate?: ModerateFn;
  track?: TrackFn;
  eventId?: string;
  action?: ModerationAction;
}) {
  const { step, trackCtx, moderate, track, eventId, action } = args;
  if (step === "authGate") return undefined;
  const context: ModerateContext = {
    trackCtx,
    moderate: moderate ?? failClosedModerateAction,
    track: track ?? defaultTrack,
    eventId: step === "queue" ? undefined : eventId,
    action:
      step === "decision" || step === "submitting" || step === "moderated"
        ? (action ?? "approve")
        : undefined,
  };
  return moderateMachine.resolveState({ value: step, context });
}
