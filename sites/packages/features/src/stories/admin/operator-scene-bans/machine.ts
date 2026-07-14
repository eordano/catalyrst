import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import { isAddress, normalizeAddress } from "@data/lib/catalyst/admin/scene-bans";

export type BanAction = "ban" | "unban";

export type { TrackFn };

export type CommitResult = { action: BanAction; address: string };

export type CommitFn = (args: {
  placeId: string;
  action: BanAction;
  address: string;
  signal?: AbortSignal;
}) => Promise<CommitResult>;

export type SceneBanInput = {
  trackCtx: TrackContext;
  placeId: string;
  total?: number;
  commit?: CommitFn;
  track?: TrackFn;
};

export type SceneBanContext = {
  trackCtx: TrackContext;
  placeId: string;
  total: number;
  commit: CommitFn;
  track: TrackFn;
  action?: BanAction;
  address?: string;
  result?: CommitResult;
  error?: string;
};

export type SceneBanEvent =
  | { type: "PICK_PLACE" }
  | { type: "START_BAN"; address: string }
  | { type: "START_UNBAN"; address: string }
  | { type: "REVIEW" }
  | { type: "SUBMIT" }
  | { type: "BACK" }
  | { type: "RESET" };

export const SCENE_BAN_EVENTS = {
  viewed: "operator_scene_bans_viewed",
  started: "operator_scene_ban_started",
  banCommitted: "operator_scene_ban_committed",
  unbanCommitted: "operator_scene_unban_committed",
  failed: "operator_scene_ban_failed",
} as const;

export const STATE_TO_SLUG = {
  pickPlace: "pick-place",
  bans: "bans",
  banOrUnban: "ban-or-unban",
  confirm: "confirm",
  submitting: "submitting",
  done: "done",
} as const;

export type SceneBanStateId = keyof typeof STATE_TO_SLUG;
export type SceneBanStepSlug = (typeof STATE_TO_SLUG)[SceneBanStateId];

export const FIRST_STEP_SLUG: SceneBanStepSlug = STATE_TO_SLUG.pickPlace;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "pickPlace");

export const SLUG_TO_STATE: Record<SceneBanStepSlug, SceneBanStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => SceneBanStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => SceneBanStateId = stepSlugs.toState;

export const simulateCommit: CommitFn = async ({ action, address, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 350);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { action, address };
};

export const sceneBanMachine = setup({
  types: {
    context: {} as SceneBanContext,
    events: {} as SceneBanEvent,
    input: {} as SceneBanInput,
  },
  actors: {
    runCommit: fromPromise<
      CommitResult,
      { placeId: string; action: BanAction; address: string; commit: CommitFn }
    >(({ input, signal }) =>
      input.commit({
        placeId: input.placeId,
        action: input.action,
        address: input.address,
        signal,
      }),
    ),
  },
  guards: {
    hasValidTarget: ({ context }) =>
      context.action === "unban"
        ? !!context.address
        : isAddress(context.address),
  },
  actions: {
    trackViewed: ({ context }) =>
      context.track(
        SCENE_BAN_EVENTS.viewed,
        { place_id: context.placeId, total: context.total },
        context.trackCtx,
      ),
    setBan: assign({
      action: () => "ban" as const,
      address: ({ event }) =>
        event.type === "START_BAN" ? normalizeAddress(event.address) : undefined,
    }),
    setUnban: assign({
      action: () => "unban" as const,
      address: ({ event }) =>
        event.type === "START_UNBAN" ? normalizeAddress(event.address) : undefined,
    }),
    trackStarted: ({ context }) =>
      context.track(
        SCENE_BAN_EVENTS.started,
        { place_id: context.placeId, action: context.action },
        context.trackCtx,
      ),
    trackCommitted: ({ context }) => {
      const event =
        context.action === "unban"
          ? SCENE_BAN_EVENTS.unbanCommitted
          : SCENE_BAN_EVENTS.banCommitted;
      context.track(
        event,
        { place_id: context.placeId, address: context.address, simulated: false },
        context.trackCtx,
      );
    },
    trackFailed: ({ context }) =>
      context.track(
        SCENE_BAN_EVENTS.failed,
        { place_id: context.placeId, action: context.action },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "sceneBanWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    placeId: input.placeId,
    total: input.total ?? 0,
    commit: input.commit ?? simulateCommit,
    track: input.track ?? defaultTrack,
  }),
  initial: "pickPlace",
  states: {
    pickPlace: {
      on: {
        PICK_PLACE: { target: "bans" },
      },
    },
    bans: {
      entry: ["trackViewed"],
      on: {
        START_BAN: { target: "banOrUnban", actions: ["setBan", "trackStarted"] },
        START_UNBAN: { target: "banOrUnban", actions: ["setUnban", "trackStarted"] },
      },
    },
    banOrUnban: {
      on: {
        REVIEW: {
          target: "confirm",
          guard: "hasValidTarget",
          actions: assign({ error: undefined }),
        },
        BACK: { target: "bans" },
      },
    },
    confirm: {
      on: {
        SUBMIT: { target: "submitting" },
        BACK: { target: "banOrUnban" },
      },
    },
    submitting: {
      invoke: {
        id: "runCommit",
        src: "runCommit",
        input: ({ context }) => ({
          placeId: context.placeId,
          action: context.action ?? "ban",
          address: context.address ?? "",
          commit: context.commit,
        }),
        onDone: {
          target: "done",
          actions: [assign({ result: ({ event }) => event.output }), "trackCommitted"],
        },
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
        RESET: {
          target: "bans",
          actions: assign({ action: undefined, address: undefined, result: undefined }),
        },
      },
    },
  },
});

export type SceneBanMachine = typeof sceneBanMachine;

export function resolveSceneBanSnapshot(args: {
  step: SceneBanStateId;
  trackCtx: TrackContext;
  placeId: string;
  total?: number;
  commit?: CommitFn;
  track?: TrackFn;
  action?: BanAction;
  address?: string;
}) {
  const { step, trackCtx, placeId, total = 0, commit, track, action, address } = args;
  if (step === "pickPlace") return undefined;
  const needsTarget =
    step === "banOrUnban" || step === "confirm" || step === "submitting" || step === "done";
  const context: SceneBanContext = {
    trackCtx,
    placeId,
    total,
    commit: commit ?? simulateCommit,
    track: track ?? defaultTrack,
    action: needsTarget ? (action ?? "ban") : undefined,
    address: needsTarget
      ? (address ?? "0x0000000000000000000000000000000000000000")
      : undefined,
  };
  return sceneBanMachine.resolveState({ value: step, context });
}
