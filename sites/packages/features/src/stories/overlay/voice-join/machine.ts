import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type VoiceKind = "private" | "community";

export type { TrackFn };

export type ConnectResult = { connectionUrl: string; roomName: string };

export type ConnectFn = (args: {
  kind: VoiceKind;
  roomId: string;
  signal?: AbortSignal;
}) => Promise<ConnectResult>;

export type VoiceInput = {
  trackCtx: TrackContext;
  connect?: ConnectFn;
  track?: TrackFn;
  roomId?: string;
};

export type VoiceContext = {
  trackCtx: TrackContext;
  connect: ConnectFn;
  track: TrackFn;
  roomId: string;
  kind?: VoiceKind;
  result?: ConnectResult;
  micMuted: boolean;
  error?: string;
};

export type VoiceEvent =
  | { type: "REQUEST"; kind: VoiceKind }
  | { type: "TOGGLE_MUTE" }
  | { type: "LEAVE" }
  | { type: "RETRY" };

export const VOICE_EVENTS = {
  widgetOpened: "cl_voice_widget_opened",
  sessionRequested: "cl_voice_session_requested",
  tokenIssued: "cl_voice_token_issued",
  join: "cl_voice_join",
  muteToggled: "cl_voice_mute_toggled",
  left: "cl_voice_left",
  sessionFailed: "cl_voice_session_failed",
} as const;

export const STATE_TO_SLUG = {
  resting: "voice",
  requesting: "request",
  connecting: "token",
  talking: "talk",
  left: "leave",
  failed: "failed",
} as const;

export type VoiceStateId = keyof typeof STATE_TO_SLUG;
export type VoiceStepSlug = (typeof STATE_TO_SLUG)[VoiceStateId] | "mute";

export const FIRST_STEP_SLUG: VoiceStepSlug = STATE_TO_SLUG.resting;

export const SLUG_TO_STATE: Record<VoiceStepSlug, VoiceStateId> = {
  voice: "resting",
  request: "requesting",
  token: "connecting",
  talk: "talking",
  mute: "talking",
  leave: "left",
  failed: "failed",
};

const MUTED_SLUGS = new Set<VoiceStepSlug>(["mute"]);

export function stateToSlug(value: string): VoiceStepSlug {
  return STATE_TO_SLUG[value as VoiceStateId] ?? FIRST_STEP_SLUG;
}

export function slugToState(slug: string | null | undefined): VoiceStateId {
  if (!slug) return "resting";
  return SLUG_TO_STATE[slug as VoiceStepSlug] ?? "resting";
}

export function slugIsMuted(slug: string | null | undefined): boolean {
  return !!slug && MUTED_SLUGS.has(slug as VoiceStepSlug);
}

export const simulateConnect: ConnectFn = async ({ kind, roomId, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const roomName =
    kind === "community"
      ? `voice-chat-community-${roomId}`
      : `voice-chat-private-${roomId}`;
  return {
    roomName,
    connectionUrl: `livekit:wss://livekit.example.com?access_token=SIMULATED.${kind.toUpperCase()}.STUB`,
  };
};

export const voiceMachine = setup({
  types: {
    context: {} as VoiceContext,
    events: {} as VoiceEvent,
    input: {} as VoiceInput,
  },
  actors: {
    runConnect: fromPromise<
      ConnectResult,
      { kind: VoiceKind; roomId: string; connect: ConnectFn }
    >(({ input, signal }) =>
      input.connect({ kind: input.kind, roomId: input.roomId, signal }),
    ),
  },
  actions: {
    trackWidgetOpened: ({ context }) =>
      context.track(VOICE_EVENTS.widgetOpened, {}, context.trackCtx),
    setKind: assign({
      kind: ({ event }) => (event.type === "REQUEST" ? event.kind : undefined),
    }),
    trackSessionRequested: ({ context, event }) => {
      if (event.type !== "REQUEST") return;
      context.track(
        VOICE_EVENTS.sessionRequested,
        { kind: event.kind },
        context.trackCtx,
      );
    },
    trackTokenIssued: ({ context }) =>
      context.track(
        VOICE_EVENTS.tokenIssued,
        { kind: context.kind, stub: true },
        context.trackCtx,
      ),
    trackJoin: ({ context }) =>
      context.track(
        VOICE_EVENTS.join,
        { kind: context.kind, room: context.result?.roomName },
        context.trackCtx,
      ),
    toggleMute: assign({ micMuted: ({ context }) => !context.micMuted }),
    trackMuteToggled: ({ context }) =>
      context.track(
        VOICE_EVENTS.muteToggled,
        { muted: context.micMuted },
        context.trackCtx,
      ),
    trackLeft: ({ context }) =>
      context.track(
        VOICE_EVENTS.left,
        { kind: context.kind, stub: true },
        context.trackCtx,
      ),
    trackSessionFailed: ({ context }) =>
      context.track(
        VOICE_EVENTS.sessionFailed,
        { kind: context.kind, error: context.error },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "voiceJoin",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    connect: input.connect ?? simulateConnect,
    track: input.track ?? defaultTrack,
    roomId: input.roomId ?? "call-9f3a21",
    micMuted: false,
  }),
  initial: "resting",
  states: {
    resting: {
      entry: "trackWidgetOpened",
      on: {
        REQUEST: {
          target: "requesting",
          actions: ["setKind", "trackSessionRequested"],
        },
      },
    },
    requesting: {
      always: { target: "connecting" },
    },
    connecting: {
      entry: [assign({ error: undefined }), "trackTokenIssued"],
      invoke: {
        id: "runConnect",
        src: "runConnect",
        input: ({ context }) => ({
          kind: context.kind ?? "private",
          roomId: context.roomId,
          connect: context.connect,
        }),
        onDone: {
          target: "talking",
          actions: assign({ result: ({ event }) => event.output }),
        },
        onError: {
          target: "failed",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "connect failed"),
          }),
        },
      },
    },
    talking: {
      entry: "trackJoin",
      on: {
        TOGGLE_MUTE: {
          actions: ["toggleMute", "trackMuteToggled"],
        },
        LEAVE: { target: "left" },
      },
    },
    left: {
      entry: "trackLeft",
      type: "final",
    },
    failed: {
      entry: "trackSessionFailed",
      on: {
        RETRY: { target: "connecting" },
        LEAVE: { target: "left" },
      },
    },
  },
});

export type VoiceMachine = typeof voiceMachine;

export function resolveVoiceSnapshot(args: {
  step: VoiceStateId;
  trackCtx: TrackContext;
  connect?: ConnectFn;
  track?: TrackFn;
  roomId?: string;
  kind?: VoiceKind;
  muted?: boolean;
}) {
  const {
    step,
    trackCtx,
    connect,
    track,
    roomId = "call-9f3a21",
    kind = "private",
    muted = false,
  } = args;
  if (step === "resting") return undefined;
  const context: VoiceContext = {
    trackCtx,
    connect: connect ?? simulateConnect,
    track: track ?? defaultTrack,
    roomId,
    kind,
    micMuted: muted,
  };
  return voiceMachine.resolveState({ value: step, context });
}
