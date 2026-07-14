import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import {
  NOT_CONNECTED_MESSAGE,
  UserActionError,
  type ActionFailureReason,
  type UserAction,
  type UserActionResult,
} from "@data/lib/catalyst/admin/user-bans";

export type { TrackFn };

export type CommitFn = (args: {
  action: UserAction;
  address: string;
  moderator: string;
  reason: string;
  durationMs?: number | null;
  customMessage?: string | null;
  signal?: AbortSignal;
}) => Promise<UserActionResult>;

export type UserBanInput = {
  trackCtx: TrackContext;
  moderator: string;
  activeAddresses: string[];
  commit?: CommitFn;
  track?: TrackFn;
};

export type UserBanContext = {
  trackCtx: TrackContext;
  moderator: string;
  activeAddresses: string[];
  commit: CommitFn;
  track: TrackFn;
  action?: UserAction;
  address?: string;
  reason?: string;
  durationMs?: number | null;
  customMessage?: string | null;
  lookupIsBanned?: boolean;
  result?: UserActionResult;
  error?: string;
  errorReason?: ActionFailureReason;
};

export type UserBanEvent =
  | { type: "SIGN_IN" }
  | { type: "LOOKUP"; address: string; isBanned: boolean }
  | {
      type: "SELECT";
      action: UserAction;
      address: string;
      reason?: string;
      durationMs?: number | null;
      customMessage?: string | null;
    }
  | { type: "REVIEW" }
  | { type: "BACK" }
  | { type: "CANCEL" }
  | { type: "COMMIT" }
  | { type: "CONTINUE" };

export const USER_BAN_EVENTS = {
  bansViewed: "operator_user_bans_viewed",
  lookup: "operator_user_ban_lookup",
  actionSelected: "operator_user_action_selected",
  banCommitted: "operator_user_ban_committed",
  warningCommitted: "operator_user_warning_committed",
  unbanCommitted: "operator_user_unban_committed",
  failed: "operator_user_ban_failed",
} as const;

export const STATE_TO_SLUG = {
  authGate: "auth-gate",
  bans: "bans",
  action: "action",
  confirm: "confirm",
  submitting: "submitting",
  done: "done",
} as const;

export type UserBanStateId = keyof typeof STATE_TO_SLUG;
export type UserBanStepSlug = (typeof STATE_TO_SLUG)[UserBanStateId];

export const FIRST_STEP_SLUG: UserBanStepSlug = STATE_TO_SLUG.authGate;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "authGate");

export const SLUG_TO_STATE: Record<UserBanStepSlug, UserBanStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => UserBanStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => UserBanStateId = stepSlugs.toState;

export const failClosedCommit: CommitFn = () =>
  Promise.reject(new Error(NOT_CONNECTED_MESSAGE));

export const userBanMachine = setup({
  types: {
    context: {} as UserBanContext,
    events: {} as UserBanEvent,
    input: {} as UserBanInput,
  },
  actors: {
    runCommit: fromPromise<
      UserActionResult,
      {
        action: UserAction;
        address: string;
        moderator: string;
        reason: string;
        durationMs?: number | null;
        customMessage?: string | null;
        commit: CommitFn;
      }
    >(({ input, signal }) =>
      input.commit({
        action: input.action,
        address: input.address,
        moderator: input.moderator,
        reason: input.reason,
        durationMs: input.durationMs,
        customMessage: input.customMessage,
        signal,
      }),
    ),
  },
  actions: {
    trackBansViewed: ({ context }) =>
      context.track(
        USER_BAN_EVENTS.bansViewed,
        { active_count: context.activeAddresses.length },
        context.trackCtx,
      ),
    setLookup: assign({
      lookupIsBanned: ({ event }) => (event.type === "LOOKUP" ? event.isBanned : undefined),
      address: ({ context, event }) =>
        event.type === "LOOKUP" ? event.address : context.address,
    }),
    trackLookup: ({ context, event }) => {
      if (event.type !== "LOOKUP") return;
      context.track(USER_BAN_EVENTS.lookup, { is_banned: event.isBanned }, context.trackCtx);
    },
    setSelection: assign({
      action: ({ event }) => (event.type === "SELECT" ? event.action : undefined),
      address: ({ event }) => (event.type === "SELECT" ? event.address : undefined),
      reason: ({ event }) => (event.type === "SELECT" ? event.reason : undefined),
      durationMs: ({ event }) => (event.type === "SELECT" ? event.durationMs : undefined),
      customMessage: ({ event }) => (event.type === "SELECT" ? event.customMessage : undefined),
      result: undefined,
      error: undefined,
      errorReason: undefined,
    }),
    trackActionSelected: ({ context, event }) => {
      if (event.type !== "SELECT") return;
      context.track(
        USER_BAN_EVENTS.actionSelected,
        { action: event.action },
        context.trackCtx,
      );
    },
    trackCommitted: ({ context }) => {
      const action = context.action ?? "ban";
      if (action === "ban") {
        context.track(
          USER_BAN_EVENTS.banCommitted,
          { has_duration: context.durationMs != null && context.durationMs > 0 },
          context.trackCtx,
        );
      } else if (action === "warn") {
        context.track(USER_BAN_EVENTS.warningCommitted, {}, context.trackCtx);
      } else {
        context.track(USER_BAN_EVENTS.unbanCommitted, {}, context.trackCtx);
      }
    },
    trackFailed: ({ context }) =>
      context.track(
        USER_BAN_EVENTS.failed,
        { action: context.action, reason: context.errorReason },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "operatorUserBans",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    moderator: input.moderator,
    activeAddresses: input.activeAddresses,
    commit: input.commit ?? failClosedCommit,
    track: input.track ?? defaultTrack,
  }),
  initial: "authGate",
  states: {
    authGate: {
      on: {
        SIGN_IN: { target: "bans" },
      },
    },
    bans: {
      entry: "trackBansViewed",
      on: {
        LOOKUP: { actions: ["setLookup", "trackLookup"] },
        SELECT: { target: "action", actions: ["setSelection", "trackActionSelected"] },
      },
    },
    action: {
      on: {
        SELECT: { actions: ["setSelection", "trackActionSelected"] },
        REVIEW: { target: "confirm" },
        BACK: { target: "bans" },
      },
    },
    confirm: {
      on: {
        COMMIT: {
          target: "submitting",
          actions: assign({ error: undefined, errorReason: undefined }),
        },
        CANCEL: { target: "action" },
      },
    },
    submitting: {
      invoke: {
        id: "runCommit",
        src: "runCommit",
        input: ({ context }) => ({
          action: context.action ?? "ban",
          address: context.address ?? "",
          moderator: context.moderator,
          reason: context.reason ?? "",
          durationMs: context.durationMs,
          customMessage: context.customMessage,
          commit: context.commit,
        }),
        onDone: {
          target: "done",
          actions: [assign({ result: ({ event }) => event.output }), "trackCommitted"],
        },
        onError: {
          target: "action",
          actions: [
            assign({
              error: ({ event }) =>
                toErrorMessage(event.error, "commit failed"),
              errorReason: ({ event }) =>
                event.error instanceof UserActionError ? event.error.reason : undefined,
            }),
            "trackFailed",
          ],
        },
      },
    },
    done: {
      on: {
        CONTINUE: {
          target: "bans",
          actions: assign({
            action: undefined,
            address: undefined,
            reason: undefined,
            durationMs: undefined,
            customMessage: undefined,
            result: undefined,
            error: undefined,
            errorReason: undefined,
            lookupIsBanned: undefined,
          }),
        },
      },
    },
  },
});

export type UserBanMachine = typeof userBanMachine;

export function resolveUserBanSnapshot(args: {
  step: UserBanStateId;
  trackCtx: TrackContext;
  moderator: string;
  activeAddresses: string[];
  commit?: CommitFn;
  track?: TrackFn;
  action?: UserAction;
  address?: string;
}) {
  const {
    step,
    trackCtx,
    moderator,
    activeAddresses,
    commit,
    track,
    action = "ban",
    address = "0x0000000000000000000000000000000000000000",
  } = args;
  if (step === "authGate") return undefined;
  const pastBans = step === "action" || step === "confirm" || step === "submitting" || step === "done";
  const context: UserBanContext = {
    trackCtx,
    moderator,
    activeAddresses,
    commit: commit ?? failClosedCommit,
    track: track ?? defaultTrack,
    action: pastBans ? action : undefined,
    address: pastBans ? address : undefined,
    reason: pastBans ? "Pre-seeded review reason for the deep-linked step." : undefined,
    durationMs: pastBans ? null : undefined,
  };
  return userBanMachine.resolveState({ value: step, context });
}
