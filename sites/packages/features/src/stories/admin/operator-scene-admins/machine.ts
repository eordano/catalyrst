import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type AdminAction = "add" | "revoke";

export type { TrackFn };

export type CommitResult = { ok: true };

export type CommitFn = (args: {
  action: AdminAction;
  placeId: string;
  admin: string;
  signal?: AbortSignal;
}) => Promise<CommitResult>;

export type ManageInput = {
  trackCtx: TrackContext;
  placeId?: string;
  commit?: CommitFn;
  track?: TrackFn;
};

export type ManageContext = {
  trackCtx: TrackContext;
  commit: CommitFn;
  track: TrackFn;
  placeId?: string;
  action?: AdminAction;
  grantAddress?: string;
  revokeAddress?: string;
  revokeCanBeRemoved?: boolean;
  error?: string;
};

export type ManageEvent =
  | { type: "SELECT_PLACE"; placeId: string }
  | { type: "START_GRANT"; address: string }
  | { type: "START_REVOKE"; address: string; canBeRemoved: boolean }
  | { type: "REVIEW" }
  | { type: "SUBMIT" }
  | { type: "BACK" }
  | { type: "DONE_BACK" }
  | { type: "CHANGE_PLACE" };

export const MANAGE_EVENTS = {
  grantStarted: "operator_admin_grant_started",
  grantCommitted: "operator_admin_grant_committed",
  revokeCommitted: "operator_admin_revoke_committed",
  actionFailed: "operator_admin_action_failed",
} as const;

export const STATE_TO_SLUG = {
  pickPlace: "pick-place",
  admins: "admins",
  grantOrRevoke: "grant-or-revoke",
  confirm: "confirm",
  submitting: "submitting",
  done: "done",
} as const;

export type ManageStateId = keyof typeof STATE_TO_SLUG;
export type ManageStepSlug = (typeof STATE_TO_SLUG)[ManageStateId];

export const FIRST_STEP_SLUG: ManageStepSlug = STATE_TO_SLUG.pickPlace;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "pickPlace");

export const SLUG_TO_STATE: Record<ManageStepSlug, ManageStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => ManageStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => ManageStateId = stepSlugs.toState;

export const simulateCommit: CommitFn = async ({ signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 350);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { ok: true };
};

export const manageMachine = setup({
  types: {
    context: {} as ManageContext,
    events: {} as ManageEvent,
    input: {} as ManageInput,
  },
  actors: {
    runCommit: fromPromise<
      CommitResult,
      { action: AdminAction; placeId: string; admin: string; commit: CommitFn }
    >(({ input, signal }) =>
      input.commit({
        action: input.action,
        placeId: input.placeId,
        admin: input.admin,
        signal,
      }),
    ),
  },
  actions: {
    setPlace: assign({
      placeId: ({ event }) =>
        event.type === "SELECT_PLACE" ? event.placeId : undefined,
    }),
    startGrant: assign({
      action: () => "add" as const,
      grantAddress: ({ event }) =>
        event.type === "START_GRANT" ? event.address.trim().toLowerCase() : undefined,
      revokeAddress: () => undefined,
      revokeCanBeRemoved: () => undefined,
    }),
    startRevoke: assign({
      action: () => "revoke" as const,
      revokeAddress: ({ event }) =>
        event.type === "START_REVOKE" ? event.address.trim().toLowerCase() : undefined,
      revokeCanBeRemoved: ({ event }) =>
        event.type === "START_REVOKE" ? event.canBeRemoved : undefined,
      grantAddress: () => undefined,
    }),
    trackStarted: ({ context }) =>
      context.track(
        MANAGE_EVENTS.grantStarted,
        { place_id: context.placeId, action: context.action },
        context.trackCtx,
      ),
    trackCommitted: ({ context }) => {
      if (context.action === "revoke") {
        context.track(
          MANAGE_EVENTS.revokeCommitted,
          {
            place_id: context.placeId,
            can_be_removed: context.revokeCanBeRemoved ?? false,
          },
          context.trackCtx,
        );
      } else {
        context.track(
          MANAGE_EVENTS.grantCommitted,
          { place_id: context.placeId },
          context.trackCtx,
        );
      }
    },
    trackFailed: ({ context }) =>
      context.track(
        MANAGE_EVENTS.actionFailed,
        { place_id: context.placeId, action: context.action },
        context.trackCtx,
      ),
  },
  guards: {
    actionReady: ({ context }) => {
      if (context.action === "add") {
        return /^0x[0-9a-fA-F]{40}$/.test(context.grantAddress ?? "");
      }
      if (context.action === "revoke") {
        return Boolean(context.revokeAddress);
      }
      return false;
    },
  },
}).createMachine({
  id: "manageAdmins",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    commit: input.commit ?? simulateCommit,
    track: input.track ?? defaultTrack,
    placeId: input.placeId,
  }),
  initial: "pickPlace",
  states: {
    pickPlace: {
      on: {
        SELECT_PLACE: { target: "admins", actions: "setPlace" },
      },
    },
    admins: {
      on: {
        START_GRANT: {
          target: "grantOrRevoke",
          actions: ["startGrant", "trackStarted"],
        },
        START_REVOKE: {
          target: "grantOrRevoke",
          actions: ["startRevoke", "trackStarted"],
        },
        CHANGE_PLACE: { target: "pickPlace" },
      },
    },
    grantOrRevoke: {
      on: {
        REVIEW: { target: "confirm", guard: "actionReady" },
        BACK: { target: "admins" },
      },
    },
    confirm: {
      on: {
        SUBMIT: { target: "submitting" },
        BACK: { target: "grantOrRevoke" },
      },
    },
    submitting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runCommit",
        src: "runCommit",
        input: ({ context }) => ({
          action: context.action ?? "add",
          placeId: context.placeId ?? "",
          admin:
            (context.action === "revoke"
              ? context.revokeAddress
              : context.grantAddress) ?? "",
          commit: context.commit,
        }),
        onDone: { target: "done", actions: "trackCommitted" },
        onError: {
          target: "confirm",
          actions: [
            assign({
              error: ({ event }) =>
                toErrorMessage(event.error, "commit failed"),
            }),
            "trackFailed",
          ],
        },
      },
    },
    done: {
      on: {
        DONE_BACK: { target: "admins" },
      },
    },
  },
});

export type ManageMachine = typeof manageMachine;

export function resolveManageSnapshot(args: {
  step: ManageStateId;
  trackCtx: TrackContext;
  placeId?: string;
  commit?: CommitFn;
  track?: TrackFn;
  action?: AdminAction;
  address?: string;
}) {
  const { step, trackCtx, placeId, commit, track, action = "add", address } = args;
  if (step === "pickPlace") return undefined;
  const isRevoke = action === "revoke";
  const context: ManageContext = {
    trackCtx,
    commit: commit ?? simulateCommit,
    track: track ?? defaultTrack,
    placeId,
    action: step === "admins" ? undefined : action,
    grantAddress: step === "admins" || isRevoke ? undefined : address,
    revokeAddress: step === "admins" || !isRevoke ? undefined : address,
    revokeCanBeRemoved: step === "admins" || !isRevoke ? undefined : true,
  };
  return manageMachine.resolveState({ value: step, context });
}
