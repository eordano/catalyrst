import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import { OPERATOR_EVENTS, trackOperator } from "@core/lib/telemetry/operator-events";
import {
  exceedsQuota,
  totalBytes,
  type DeployFile,
} from "@data/lib/catalyst/creator-hub/deploy-world";

export type { TrackFn };

export type DeployResult = { jumpUrl: string };

export type DeployTarget = "world" | "land";

export type DeployLand = { parcels: string[]; baseParcel: string };

export type DeployFn = (args: {
  name: string;
  target: DeployTarget;
  signal?: AbortSignal;
}) => Promise<DeployResult>;

export type DeployInput = {
  trackCtx: TrackContext;
  files?: DeployFile[];
  maxFileSizeMb?: number;
  namesEmpty?: boolean;
  defaultName?: string;
  land?: DeployLand | null;
  initialTarget?: DeployTarget;
  deploy?: DeployFn;
  track?: TrackFn;
};

export type DeployContext = {
  trackCtx: TrackContext;
  files: DeployFile[];
  maxFileSizeMb: number;
  namesEmpty: boolean;
  land?: DeployLand | null;
  target: DeployTarget;
  deploy?: DeployFn;
  track: TrackFn;
  name?: string;
  result?: DeployResult;
  error?: string;
  quotaError?: boolean;
};

export type DeployEvent =
  | { type: "CHOOSE_WORLDS" }
  | { type: "CHOOSE_LAND" }
  | { type: "PICK_NAME"; name: string }
  | { type: "SET_FILES"; files: DeployFile[] }
  | { type: "CONFIRM" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const DEPLOY_EVENTS = {
  started: "ch_deploy_world_started",
  destinationSelected: "ch_deploy_world_destination_selected",
  namesEmpty: "ch_deploy_world_names_empty",
  nameSelected: "ch_deploy_world_name_selected",
  reviewReached: "ch_deploy_world_review_reached",
  quotaExceeded: "ch_deploy_world_quota_exceeded",
  confirmReached: "ch_deploy_world_confirm_reached",
  completed: "ch_deploy_world_completed",
  failed: "ch_deploy_world_failed",
} as const;

export const STATE_TO_SLUG = {
  destination: "destination",
  selectWorld: "select-world",
  namesEmpty: "select-empty",
  review: "review",
  unavailable: "unavailable",
  deploying: "deploying",
  finishing: "finishing",
  complete: "complete",
  error: "error",
} as const;

export type DeployStateId = keyof typeof STATE_TO_SLUG;
export type DeployStepSlug = (typeof STATE_TO_SLUG)[DeployStateId];

export const FIRST_STEP_SLUG: DeployStepSlug = STATE_TO_SLUG.destination;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "destination");

export const SLUG_TO_STATE: Record<DeployStepSlug, DeployStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => DeployStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => DeployStateId = stepSlugs.toState;

const DEFAULT_NAME = "mystore.dcl.eth";

export const deployWorldMachine = setup({
  types: {
    context: {} as DeployContext,
    events: {} as DeployEvent,
    input: {} as DeployInput,
  },
  actors: {
    runDeploy: fromPromise<
      DeployResult,
      { name: string; target: DeployTarget; deploy: DeployFn }
    >(({ input, signal }) =>
      input.deploy({ name: input.name, target: input.target, signal }),
    ),
  },
  guards: {
    hasNames: ({ context }) => !context.namesEmpty,
    hasLandChoice: ({ context }) => !!context.land,
    isLandTarget: ({ context }) => context.target === "land",
    autoWorld: ({ context }) => !context.land && !context.namesEmpty,
    autoNamesEmpty: ({ context }) => !context.land && context.namesEmpty,
    hasRealDeploy: ({ context }) => !!context.deploy,
    overQuota: ({ context }) => exceedsQuota(context.files, context.maxFileSizeMb),
  },
  actions: {
    setWorldTarget: assign({ target: "world" as DeployTarget }),
    setLandTarget: assign({ target: "land" as DeployTarget }),
    trackStarted: ({ context, event }) => {
      const target: DeployTarget = event.type === "CHOOSE_LAND" ? "land" : "world";
      context.track(DEPLOY_EVENTS.started, { target }, context.trackCtx);
      context.track(
        DEPLOY_EVENTS.destinationSelected,
        { target },
        context.trackCtx,
      );
      trackOperator(
        OPERATOR_EVENTS.deployStarted,
        target,
        {},
        context.trackCtx,
        context.track,
      );
    },
    trackLandSelected: ({ context }) => {
      trackOperator(
        OPERATOR_EVENTS.placementValidated,
        "land",
        { parcels: context.land?.parcels.length ?? 0 },
        context.trackCtx,
        context.track,
      );
    },
    trackNamesEmpty: ({ context }) =>
      context.track(DEPLOY_EVENTS.namesEmpty, {}, context.trackCtx),
    setName: assign({
      name: ({ context, event }) =>
        event.type === "PICK_NAME" ? event.name : context.name,
    }),
    trackNameSelected: ({ context }) => {
      context.track(
        DEPLOY_EVENTS.nameSelected,
        { name: context.name },
        context.trackCtx,
      );
      trackOperator(
        OPERATOR_EVENTS.placementValidated,
        "world",
        { name: context.name },
        context.trackCtx,
        context.track,
      );
    },
    trackReviewReached: ({ context }) => {
      const total = totalBytes(context.files);
      const exceeded = exceedsQuota(context.files, context.maxFileSizeMb);
      context.track(
        DEPLOY_EVENTS.reviewReached,
        { total_bytes: total, exceeded, target: context.target },
        context.trackCtx,
      );
      if (exceeded) {
        context.track(
          DEPLOY_EVENTS.quotaExceeded,
          { total_bytes: total, max_mb: context.maxFileSizeMb },
          context.trackCtx,
        );
        trackOperator(
          OPERATOR_EVENTS.placementRejected,
          context.target,
          { reason: "quota_exceeded", total_bytes: total },
          context.trackCtx,
          context.track,
        );
      }
    },
    trackConfirmReached: ({ context }) =>
      context.track(
        DEPLOY_EVENTS.confirmReached,
        { name: context.name, target: context.target },
        context.trackCtx,
      ),
    flagQuotaBlocked: assign({ quotaError: true }),
    trackQuotaBlocked: ({ context }) => {
      context.track(
        DEPLOY_EVENTS.quotaExceeded,
        {
          total_bytes: totalBytes(context.files),
          max_mb: context.maxFileSizeMb,
          phase: "confirm",
        },
        context.trackCtx,
      );
      trackOperator(
        OPERATOR_EVENTS.placementRejected,
        context.target,
        { reason: "quota_exceeded", phase: "confirm" },
        context.trackCtx,
        context.track,
      );
    },
    trackCompleted: ({ context }) => {
      context.track(
        DEPLOY_EVENTS.completed,
        {
          name: context.name,
          target: context.target,
          jump_url: context.result?.jumpUrl,
        },
        context.trackCtx,
      );
      trackOperator(
        OPERATOR_EVENTS.deployCompleted,
        context.target,
        { name: context.name },
        context.trackCtx,
        context.track,
      );
    },
    trackFailed: ({ context }) =>
      context.track(
        DEPLOY_EVENTS.failed,
        { name: context.name, error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "deployWorldWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    files: input.files ?? [],
    maxFileSizeMb: input.maxFileSizeMb ?? 50,
    namesEmpty: input.namesEmpty ?? false,
    land: input.land ?? null,
    target: input.initialTarget ?? "world",
    deploy: input.deploy,
    track: input.track ?? defaultTrack,
    name: input.defaultName,
  }),
  initial: "destination",
  on: {
    SET_FILES: {
      actions: assign({ files: ({ event }) => event.files }),
    },
  },
  states: {
    destination: {
      always: [
        { target: "selectWorld", guard: "autoWorld", actions: "trackStarted" },
        {
          target: "namesEmpty",
          guard: "autoNamesEmpty",
          actions: ["trackStarted", "trackNamesEmpty"],
        },
      ],
      on: {
        CHOOSE_WORLDS: [
          {
            target: "selectWorld",
            guard: "hasNames",
            actions: ["setWorldTarget", "trackStarted"],
          },
          {
            target: "namesEmpty",
            actions: ["setWorldTarget", "trackStarted", "trackNamesEmpty"],
          },
        ],
        CHOOSE_LAND: {
          target: "review",
          guard: "hasLandChoice",
          actions: ["setLandTarget", "trackStarted", "trackLandSelected"],
        },
      },
    },
    selectWorld: {
      on: {
        PICK_NAME: { target: "review", actions: ["setName", "trackNameSelected"] },
        BACK: { target: "destination" },
      },
    },
    namesEmpty: {
      on: {
        BACK: { target: "destination" },
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        CONFIRM: [
          { guard: "overQuota", actions: ["flagQuotaBlocked", "trackQuotaBlocked"] },
          { target: "deploying", guard: "hasRealDeploy" },
          { target: "unavailable" },
        ],
        BACK: [
          { target: "destination", guard: "isLandTarget" },
          { target: "selectWorld" },
        ],
      },
    },
    unavailable: {
      type: "final",
    },
    deploying: {
      entry: [assign({ error: undefined }), "trackConfirmReached"],
      invoke: {
        id: "runDeploy",
        src: "runDeploy",
        input: ({ context }) => ({
          name:
            context.target === "land"
              ? (context.land?.baseParcel ?? context.land?.parcels[0] ?? "")
              : (context.name ?? DEFAULT_NAME),
          target: context.target,
          deploy: context.deploy as DeployFn,
        }),
        onDone: {
          target: "finishing",
          actions: assign({ result: ({ event }) => event.output }),
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "publish failed"),
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
      entry: "trackFailed",
      on: {
        RETRY: { target: "deploying" },
      },
    },
  },
});

export type DeployWorldMachine = typeof deployWorldMachine;

export function resolveDeploySnapshot(args: {
  step: DeployStateId;
  trackCtx: TrackContext;
  files?: DeployFile[];
  maxFileSizeMb?: number;
  namesEmpty?: boolean;
  land?: DeployLand | null;
  target?: DeployTarget;
  deploy?: DeployFn;
  track?: TrackFn;
  name?: string;
}) {
  const {
    step,
    trackCtx,
    files = [],
    maxFileSizeMb = 50,
    namesEmpty = false,
    land = null,
    target = "world",
    deploy,
    track,
    name = DEFAULT_NAME,
  } = args;
  if (step === "destination") return undefined;
  const context: DeployContext = {
    trackCtx,
    files,
    maxFileSizeMb,
    namesEmpty,
    land,
    target,
    deploy,
    track: track ?? defaultTrack,
    name,
  };
  return deployWorldMachine.resolveState({ value: step, context });
}
