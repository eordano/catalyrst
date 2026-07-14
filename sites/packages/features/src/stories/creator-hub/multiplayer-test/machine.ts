import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  validateSpec,
  type MpLaunchRequest,
} from "@ui/creatorhub/mp/rules";
import {
  normalizeReplayOutcome,
  type MpReplayOutcome,
  type MpRunPhase,
} from "@ui/creatorhub/mp/types";

export type MpTrackFn = (
  event: string,
  props: Record<string, unknown>,
  ctx: TrackContext,
) => void;

export type MpLaunchFn = (req: MpLaunchRequest) => Promise<{ id: string }>;

export type MpReplayFn = (args: {
  runId: string;
  tier: "a" | "b";
  profile?: string;
  seed?: number;
}) => Promise<unknown>;

export type MpStopFn = (runId: string) => void;

export const MP_EVENTS = {
  paired: "ch_mp_paired",
  launched: "ch_mp_run_launched",
  rejected: "ch_mp_run_rejected",
  completed: "ch_mp_run_completed",
  failed: "ch_mp_run_failed",
  replayRequested: "ch_mp_replay_requested",
  replayCompleted: "ch_mp_replay_completed",
  replayFailed: "ch_mp_replay_failed",
} as const;

export type MpPanelInput = {
  trackCtx: TrackContext;
  track: MpTrackFn;
  launch: MpLaunchFn;
  replay: MpReplayFn;
  stop?: MpStopFn;
};

export type MpPanelContext = {
  trackCtx: TrackContext;
  track: MpTrackFn;
  launch: MpLaunchFn;
  replay: MpReplayFn;
  stop?: MpStopFn;
  runId: string | null;
  phase: MpRunPhase | null;
  detail: string | null;
  pendingSpec: MpLaunchRequest | null;
  lastError: string | null;
  replayOutcome: MpReplayOutcome | null;
  replayError: string | null;
};

export type MpPanelEvent =
  | { type: "PAIRED" }
  | { type: "PAIR_LOST" }
  | { type: "LAUNCH"; spec: MpLaunchRequest }
  | { type: "STATUS"; state: string; detail?: string }
  | { type: "SELECT_RUN"; runId: string }
  | { type: "STOP" }
  | { type: "NEW_RUN" }
  | { type: "OPEN_REPLAY" }
  | { type: "CLOSE_REPLAY" }
  | { type: "REPLAY"; tier: "a" | "b"; profile?: string; seed?: number };

const errText = (e: unknown): string =>
  toErrorMessage(e, "unknown error");

function specProps(req: MpLaunchRequest): Record<string, unknown> {
  if ("preset" in req) return { preset: req.preset };
  return {
    lane: req.lane,
    bots: req.bots,
    mode: req.mode,
    source_kind: req.scene.kind,
    profile: req.shape?.profile,
  };
}

export const mpPanelMachine = setup({
  types: {
    context: {} as MpPanelContext,
    events: {} as MpPanelEvent,
    input: {} as MpPanelInput,
  },
  actors: {
    runLaunch: fromPromise<
      { id: string },
      { req: MpLaunchRequest; launch: MpLaunchFn }
    >(({ input }) => input.launch(input.req)),
    runReplay: fromPromise<
      MpReplayOutcome,
      {
        runId: string;
        tier: "a" | "b";
        profile?: string;
        seed?: number;
        replay: MpReplayFn;
      }
    >(async ({ input }) => {
      const raw = await input.replay({
        runId: input.runId,
        tier: input.tier,
        profile: input.profile,
        seed: input.seed,
      });
      return normalizeReplayOutcome(raw, input.tier);
    }),
  },
  guards: {
    launchable: ({ event }) =>
      event.type === "LAUNCH" && validateSpec(event.spec).ok,
    statusDone: ({ event }) => event.type === "STATUS" && event.state === "done",
    statusFailed: ({ event }) =>
      event.type === "STATUS" && event.state === "failed",
  },
  actions: {
    trackPaired: ({ context }) =>
      context.track(MP_EVENTS.paired, {}, context.trackCtx),
    trackLaunched: ({ context }) => {
      if (context.pendingSpec) {
        context.track(
          MP_EVENTS.launched,
          specProps(context.pendingSpec),
          context.trackCtx,
        );
      }
    },
    trackCompleted: ({ context }) =>
      context.track(MP_EVENTS.completed, { run: context.runId }, context.trackCtx),
    callStop: ({ context }) => {
      if (context.runId) context.stop?.(context.runId);
    },
  },
}).createMachine({
  id: "mpPanel",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    track: input.track,
    launch: input.launch,
    replay: input.replay,
    stop: input.stop,
    runId: null,
    phase: null,
    detail: null,
    pendingSpec: null,
    lastError: null,
    replayOutcome: null,
    replayError: null,
  }),
  initial: "unpaired",
  on: {
    PAIR_LOST: { target: ".unpaired" },
  },
  states: {
    unpaired: {
      on: {
        PAIRED: { target: "idle", actions: "trackPaired" },
      },
    },
    idle: {
      on: {
        LAUNCH: {
          target: "launching",
          guard: "launchable",
          actions: assign({
            pendingSpec: ({ event }) => event.spec,
            lastError: null,
          }),
        },
        SELECT_RUN: {
          target: "reviewing",
          actions: assign({
            runId: ({ event }) => event.runId,
            phase: "done" as MpRunPhase,
            detail: null,
          }),
        },
      },
    },
    launching: {
      entry: "trackLaunched",
      invoke: {
        id: "runLaunch",
        src: "runLaunch",
        input: ({ context }) => ({
          req: context.pendingSpec as MpLaunchRequest,
          launch: context.launch,
        }),
        onDone: {
          target: "running",
          actions: assign({
            runId: ({ event }) => event.output.id,
            phase: "queued" as MpRunPhase,
            detail: null,
          }),
        },
        onError: {
          target: "idle",
          actions: [
            assign({ lastError: ({ event }) => errText(event.error) }),
            ({ context, event }) =>
              context.track(
                MP_EVENTS.rejected,
                { error: errText(event.error) },
                context.trackCtx,
              ),
          ],
        },
      },
    },
    running: {
      on: {
        STATUS: [
          {
            guard: "statusDone",
            target: "reviewing",
            actions: [
              assign({
                phase: "done" as MpRunPhase,
                detail: ({ event }) =>
                  event.type === "STATUS" ? (event.detail ?? null) : null,
              }),
              "trackCompleted",
            ],
          },
          {
            guard: "statusFailed",
            target: "idle",
            actions: [
              assign({
                phase: "failed" as MpRunPhase,
                lastError: ({ event }) =>
                  event.type === "STATUS"
                    ? (event.detail ?? "run failed")
                    : "run failed",
              }),
              ({ context, event }) =>
                context.track(
                  MP_EVENTS.failed,
                  {
                    run: context.runId,
                    detail: event.type === "STATUS" ? event.detail : undefined,
                  },
                  context.trackCtx,
                ),
            ],
          },
          {
            actions: assign({
              phase: ({ event, context }) =>
                event.type === "STATUS"
                  ? (event.state as MpRunPhase)
                  : context.phase,
              detail: ({ event, context }) =>
                event.type === "STATUS" ? (event.detail ?? null) : context.detail,
            }),
          },
        ],
        STOP: { actions: "callStop" },
      },
    },
    reviewing: {
      initial: "report",
      on: {
        NEW_RUN: {
          target: "idle",
          actions: assign({
            runId: null,
            phase: null,
            detail: null,
            pendingSpec: null,
            replayOutcome: null,
            replayError: null,
          }),
        },
        SELECT_RUN: {
          target: "reviewing",
          reenter: true,
          actions: assign({
            runId: ({ event }) => event.runId,
            replayOutcome: null,
            replayError: null,
          }),
        },
      },
      states: {
        report: {
          on: {
            OPEN_REPLAY: {
              target: "replay",
              actions: assign({ replayOutcome: null, replayError: null }),
            },
          },
        },
        replay: {
          initial: "form",
          on: {
            CLOSE_REPLAY: { target: "report" },
          },
          states: {
            form: {
              on: {
                REPLAY: { target: "requesting" },
              },
            },
            requesting: {
              entry: ({ context, event }) =>
                context.track(
                  MP_EVENTS.replayRequested,
                  {
                    run: context.runId,
                    tier: event.type === "REPLAY" ? event.tier : undefined,
                  },
                  context.trackCtx,
                ),
              invoke: {
                id: "runReplay",
                src: "runReplay",
                input: ({ context, event }) => ({
                  runId: context.runId as string,
                  tier: event.type === "REPLAY" ? event.tier : "a",
                  profile: event.type === "REPLAY" ? event.profile : undefined,
                  seed: event.type === "REPLAY" ? event.seed : undefined,
                  replay: context.replay,
                }),
                onDone: {
                  target: "result",
                  actions: [
                    assign({ replayOutcome: ({ event }) => event.output }),
                    ({ context, event }) =>
                      context.track(
                        MP_EVENTS.replayCompleted,
                        { run: context.runId, tier: event.output.tier },
                        context.trackCtx,
                      ),
                  ],
                },
                onError: {
                  target: "form",
                  actions: [
                    assign({ replayError: ({ event }) => errText(event.error) }),
                    ({ context, event }) =>
                      context.track(
                        MP_EVENTS.replayFailed,
                        { run: context.runId, error: errText(event.error) },
                        context.trackCtx,
                      ),
                  ],
                },
              },
            },
            result: {
              on: {
                REPLAY: { target: "requesting" },
              },
            },
          },
        },
      },
    },
  },
});

export type MpPanelMachine = typeof mpPanelMachine;
