import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import { isValidAddress } from "@data/lib/catalyst/creator-hub/world-permissions";

export type AccessType = "unrestricted" | "allow-list" | "shared-secret";

export type InviteChannel = "wallet" | "community" | "csv";

export type { TrackFn };

export type CommitResult = {
  aclWritten: true;
  addresses: number;
  stub?: boolean;
};

export type CommitFn = (args: {
  accessType: AccessType;
  collaborators: string[];
  signal?: AbortSignal;
}) => Promise<CommitResult>;

export type PermissionsInput = {
  trackCtx: TrackContext;
  accessType?: AccessType;
  collaborators?: string[];
  commit?: CommitFn;
  track?: TrackFn;
};

export type PermissionsContext = {
  trackCtx: TrackContext;
  track: TrackFn;
  commit: CommitFn;
  accessType: AccessType;
  collaborators: string[];
  candidate?: string;
  addressError?: string;
  result?: CommitResult;
  error?: string;
};

export type PermissionsEvent =
  | { type: "START_INVITE" }
  | { type: "SUBMIT_INVITE"; channel: InviteChannel }
  | { type: "OPEN_PASSWORD" }
  | { type: "SET_PASSWORD" }
  | { type: "ADD" }
  | { type: "VALIDATE"; address: string }
  | { type: "CANCEL" }
  | { type: "CONFIRM" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const PERMS_EVENTS = {
  started: "ch_world_perms_started",
  accessTypeSet: "ch_world_perms_access_type_set",
  inviteSubmitted: "ch_world_perms_invite_submitted",
  passwordSet: "ch_world_perms_password_set",
  collaboratorValidated: "ch_world_perms_collaborator_validated",
  invalidAddress: "ch_world_perms_invalid_address",
  confirmReached: "ch_world_perms_confirm_reached",
  completed: "ch_world_perms_completed",
} as const;

export const STATE_TO_SLUG = {
  access: "access",
  invite: "invite",
  password: "password",
  collaborators: "collaborators",
  addingCollaborator: "add-collaborator",
  confirming: "confirm",
  finishing: "finishing",
  complete: "complete",
  error: "error",
} as const;

export type PermsStateId = keyof typeof STATE_TO_SLUG;
export type PermsStepSlug = (typeof STATE_TO_SLUG)[PermsStateId];

export const FIRST_STEP_SLUG: PermsStepSlug = STATE_TO_SLUG.access;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "access");

export const SLUG_TO_STATE: Record<PermsStepSlug, PermsStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => PermsStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => PermsStateId = stepSlugs.toState;

const INVALID_ADDRESS = "Invalid address";

export const simulateCommit: CommitFn = async ({ collaborators, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 350);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { aclWritten: true, addresses: collaborators.length, stub: true };
};

export const permissionsMachine = setup({
  types: {
    context: {} as PermissionsContext,
    events: {} as PermissionsEvent,
    input: {} as PermissionsInput,
  },
  actors: {
    runCommit: fromPromise<
      CommitResult,
      { accessType: AccessType; collaborators: string[]; commit: CommitFn }
    >(({ input, signal }) =>
      input.commit({
        accessType: input.accessType,
        collaborators: input.collaborators,
        signal,
      }),
    ),
  },
  guards: {
    isValidCandidate: ({ event }) =>
      event.type === "VALIDATE" && isValidAddress(event.address),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(PERMS_EVENTS.started, {}, context.trackCtx),
    trackInviteSubmitted: ({ context, event }) => {
      if (event.type !== "SUBMIT_INVITE") return;
      context.track(
        PERMS_EVENTS.inviteSubmitted,
        { channel: event.channel },
        context.trackCtx,
      );
    },
    trackPasswordSet: ({ context }) => {
      context.track(PERMS_EVENTS.passwordSet, {}, context.trackCtx);
      context.track(
        PERMS_EVENTS.accessTypeSet,
        { access_type: "shared-secret" },
        context.trackCtx,
      );
    },
    enterAddDialog: assign({ candidate: undefined, addressError: undefined }),
    acceptCandidate: assign({
      collaborators: ({ context, event }) =>
        event.type === "VALIDATE"
          ? [...context.collaborators, event.address.trim()]
          : context.collaborators,
      candidate: undefined,
      addressError: undefined,
    }),
    rejectCandidate: assign({
      candidate: ({ event }) =>
        event.type === "VALIDATE" ? event.address : undefined,
      addressError: INVALID_ADDRESS,
    }),
    trackValidatedOk: ({ context, event }) =>
      context.track(
        PERMS_EVENTS.collaboratorValidated,
        { valid: true, address: event.type === "VALIDATE" ? event.address : undefined },
        context.trackCtx,
      ),
    trackValidatedFail: ({ context }) => {
      context.track(
        PERMS_EVENTS.collaboratorValidated,
        { valid: false },
        context.trackCtx,
      );
      context.track(PERMS_EVENTS.invalidAddress, {}, context.trackCtx);
    },
    trackConfirmReached: ({ context }) =>
      context.track(
        PERMS_EVENTS.confirmReached,
        { access_type: context.accessType, collaborators: context.collaborators.length },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        PERMS_EVENTS.completed,
        {
          access_type: context.accessType,
          addresses: context.result?.addresses,
          stub: context.result?.stub ?? false,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "worldPermissionsWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    track: input.track ?? defaultTrack,
    commit: input.commit ?? simulateCommit,
    accessType: input.accessType ?? "allow-list",
    collaborators: input.collaborators ?? [],
  }),
  initial: "access",
  states: {
    access: {
      on: {
        START_INVITE: { target: "invite", actions: "trackStarted" },
        OPEN_PASSWORD: { target: "password" },
      },
    },
    invite: {
      on: {
        SUBMIT_INVITE: { target: "collaborators", actions: "trackInviteSubmitted" },
        BACK: { target: "access" },
      },
    },
    password: {
      on: {
        SET_PASSWORD: { target: "access", actions: "trackPasswordSet" },
        BACK: { target: "access" },
      },
    },
    collaborators: {
      on: {
        ADD: { target: "addingCollaborator", actions: "enterAddDialog" },
        CONFIRM: { target: "confirming" },
        BACK: { target: "access" },
      },
    },
    addingCollaborator: {
      on: {
        VALIDATE: [
          {
            guard: "isValidCandidate",
            target: "collaborators",
            actions: ["acceptCandidate", "trackValidatedOk"],
          },
          {
            target: "addingCollaborator",
            actions: ["rejectCandidate", "trackValidatedFail"],
          },
        ],
        CANCEL: { target: "collaborators" },
      },
    },
    confirming: {
      entry: [assign({ error: undefined }), "trackConfirmReached"],
      invoke: {
        id: "runCommit",
        src: "runCommit",
        input: ({ context }) => ({
          accessType: context.accessType,
          collaborators: context.collaborators,
          commit: context.commit,
        }),
        onDone: {
          target: "finishing",
          actions: assign({ result: ({ event }) => event.output }),
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "acl write failed"),
          }),
        },
      },
    },
    finishing: {
      entry: "trackCompleted",
      after: {
        500: { target: "complete" },
      },
    },
    complete: {
      type: "final",
    },
    error: {
      on: {
        RETRY: { target: "confirming" },
      },
    },
  },
});

export type PermissionsMachine = typeof permissionsMachine;

export function resolvePermissionsSnapshot(args: {
  step: PermsStateId;
  trackCtx: TrackContext;
  accessType?: AccessType;
  collaborators?: string[];
  commit?: CommitFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, accessType = "allow-list", collaborators = [], commit, track } = args;
  if (step === "access") return undefined;
  const context: PermissionsContext = {
    trackCtx,
    track: track ?? defaultTrack,
    commit: commit ?? simulateCommit,
    accessType,
    collaborators,
  };
  return permissionsMachine.resolveState({ value: step, context });
}
