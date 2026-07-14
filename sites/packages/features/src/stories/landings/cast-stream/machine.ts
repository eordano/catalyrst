import { delay, makeStepSlugs } from "@core/lib/stories/index";
import { toErrorMessage } from "@core/lib/errors";
import { assign, fromPromise, setup } from "xstate";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type StreamInfo = {
  placeName: string;
  placeId: string;
  location: string;
  isWorld: boolean;
};

export type StreamerCredentials = {
  url: string;
  token: string;
  roomId: string;
  identity: string;
};

export type DeviceSelection = {
  mic: string;
  speaker: string;
  camera: string;
};

export type TokenResult = { info: StreamInfo; credentials: StreamerCredentials };

export type ResolveTokenFn = (args: {
  token: string;
  identity: string;
  signal?: AbortSignal;
}) => Promise<TokenResult>;

export type RequestPermissionsFn = (args: {
  devices: DeviceSelection;
  signal?: AbortSignal;
}) => Promise<{ granted: boolean }>;

export type EndCastFn = (args: { signal?: AbortSignal }) => Promise<void>;

export type ShareScreenFn = (args: {
  signal?: AbortSignal;
}) => Promise<{ published: boolean }>;

export type CastInput = {
  trackCtx: TrackContext;
  token: string;
  identity?: string;
  resolveToken?: ResolveTokenFn;
  requestPermissions?: RequestPermissionsFn;
  endCast?: EndCastFn;
  shareScreen?: ShareScreenFn;
  track?: TrackFn;
};

export type CastContext = {
  trackCtx: TrackContext;
  token: string;
  identity: string;
  resolveToken: ResolveTokenFn;
  requestPermissions: RequestPermissionsFn;
  endCast: EndCastFn;
  shareScreen: ShareScreenFn;
  track: TrackFn;
  info?: StreamInfo;
  credentials?: StreamerCredentials;
  devices: DeviceSelection;
  permissionsDenied: boolean;
  screenSharing: boolean;
  screenShareFailed: boolean;
  invalidReason?: string;
};

export type CastEvent =
  | { type: "SELECT_DEVICES"; devices: DeviceSelection }
  | { type: "GRANT" }
  | { type: "RETRY_PERMISSIONS" }
  | { type: "JOIN" }
  | { type: "BACK" }
  | { type: "TOGGLE_SCREENSHARE" }
  | { type: "LEAVE" }
  | { type: "RETRY" };

export const CAST_EVENTS = {
  tokenChecked: "cast_token_checked",
  tokenValid: "cast_token_valid",
  invalidToken: "cast_invalid_token",
  devicesSelected: "cast_devices_selected",
  permissionsGranted: "cast_permissions_granted",
  permissionsDenied: "cast_permissions_denied",
  previewReady: "cast_preview_ready",
  joinRequested: "cast_join_requested",
  wentLive: "cast_went_live",
  screenshareStarted: "cast_screenshare_started",
  screenshareFailed: "cast_screenshare_failed",
  ending: "cast_ending",
  ended: "cast_ended",
} as const;

export const STATE_TO_SLUG = {
  tokenCheck: "token-check",
  deviceSelect: "device-select",
  permissions: "permissions",
  preview: "preview",
  live: "live",
  ending: "ending",
  ended: "ended",
  invalid: "invalid",
} as const;

export type CastStateId = keyof typeof STATE_TO_SLUG;
export type CastStepSlug = (typeof STATE_TO_SLUG)[CastStateId];

export const FIRST_STEP_SLUG: CastStepSlug = STATE_TO_SLUG.tokenCheck;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "tokenCheck");

export const SLUG_TO_STATE: Record<CastStepSlug, CastStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => CastStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => CastStateId = stepSlugs.toState;

export const DEFAULT_DEVICES: DeviceSelection = {
  mic: "Default - Microphone",
  speaker: "Default - Speakers",
  camera: "FaceTime HD Camera",
};

const SIM_STREAM_INFO: StreamInfo = {
  placeName: "Genesis Plaza",
  placeId: "0f5eddf5-d79f-4129-9456-28f0f9cb47f3",
  location: "0,0",
  isWorld: false,
};

export const simulateResolveToken: ResolveTokenFn = async ({
  token,
  identity,
  signal,
}) => {
  await delay(350, signal);
  const t = token.trim();
  if (!t) throw new Error("Invalid streaming key");
  if (t.toLowerCase() === "expired") throw new Error("Streaming token has expired");
  return {
    info: SIM_STREAM_INFO,
    credentials: {
      url: "wss://livekit.decentraland.org",
      token: `SIMULATED.${btoaSafe(`${identity || "Speaker"}:${SIM_STREAM_INFO.location}`)}.stub-livekit-jwt`,
      roomId: `scene-${SIM_STREAM_INFO.location}-${slugifyRoom(SIM_STREAM_INFO.placeName)}`,
      identity: identity || "Speaker",
    },
  };
};

export const simulateGrant: RequestPermissionsFn = async ({ signal }) => {
  await delay(250, signal);
  return { granted: true };
};

export const simulateEndCast: EndCastFn = async ({ signal }) => {
  await delay(300, signal);
};

export const simulateShareScreen: ShareScreenFn = async ({ signal }) => {
  await delay(200, signal);
  return { published: true };
};

function btoaSafe(s: string): string {
  try {
    if (typeof btoa === "function") {
      const bytes = new TextEncoder().encode(s);
      let binary = "";
      for (const byte of bytes) binary += String.fromCharCode(byte);
      return btoa(binary);
    }
  } catch {
  }
  return s;
}

function slugifyRoom(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
}

export const castMachine = setup({
  types: {
    context: {} as CastContext,
    events: {} as CastEvent,
    input: {} as CastInput,
  },
  actors: {
    resolveToken: fromPromise<
      TokenResult,
      { token: string; identity: string; resolveToken: ResolveTokenFn }
    >(({ input, signal }) =>
      input.resolveToken({ token: input.token, identity: input.identity, signal }),
    ),
    requestPermissions: fromPromise<
      { granted: boolean },
      { devices: DeviceSelection; requestPermissions: RequestPermissionsFn }
    >(({ input, signal }) =>
      input.requestPermissions({ devices: input.devices, signal }),
    ),
    endCast: fromPromise<void, { endCast: EndCastFn }>(({ input, signal }) =>
      input.endCast({ signal }),
    ),
    shareScreen: fromPromise<{ published: boolean }, { shareScreen: ShareScreenFn }>(
      ({ input, signal }) => input.shareScreen({ signal }),
    ),
  },
  actions: {
    trackTokenChecked: ({ context }) =>
      context.track(CAST_EVENTS.tokenChecked, {}, context.trackCtx),
    setTokenResult: assign({
      info: ({ event }) =>
        "output" in event ? (event.output as TokenResult).info : undefined,
      credentials: ({ event }) =>
        "output" in event ? (event.output as TokenResult).credentials : undefined,
    }),
    trackTokenValid: ({ context }) =>
      context.track(
        CAST_EVENTS.tokenValid,
        { place_name: context.info?.placeName, is_world: context.info?.isWorld },
        context.trackCtx,
      ),
    setInvalidReason: assign({
      invalidReason: ({ event }) => {
        if ("error" in event) {
          const e = (event as { error: unknown }).error;
          return toErrorMessage(e, "Invalid streaming key");
        }
        return "Invalid streaming key";
      },
    }),
    trackInvalidToken: ({ context }) =>
      context.track(
        CAST_EVENTS.invalidToken,
        { reason: context.invalidReason ?? "Invalid streaming key" },
        context.trackCtx,
      ),
    setDevices: assign({
      devices: ({ context, event }) =>
        event.type === "SELECT_DEVICES" ? event.devices : context.devices,
    }),
    trackDevicesSelected: ({ context }) =>
      context.track(
        CAST_EVENTS.devicesSelected,
        {
          mic: context.devices.mic,
          speaker: context.devices.speaker,
          camera: context.devices.camera,
        },
        context.trackCtx,
      ),
    clearDenied: assign({ permissionsDenied: false }),
    setDenied: assign({ permissionsDenied: true }),
    trackPermissionsGranted: ({ context }) =>
      context.track(CAST_EVENTS.permissionsGranted, {}, context.trackCtx),
    trackPermissionsDenied: ({ context }) =>
      context.track(CAST_EVENTS.permissionsDenied, {}, context.trackCtx),
    trackPreviewReady: ({ context }) =>
      context.track(CAST_EVENTS.previewReady, {}, context.trackCtx),
    trackJoinRequested: ({ context }) =>
      context.track(CAST_EVENTS.joinRequested, {}, context.trackCtx),
    trackWentLive: ({ context }) =>
      context.track(
        CAST_EVENTS.wentLive,
        { room_id: context.credentials?.roomId, stub: true },
        context.trackCtx,
      ),
    setSharing: assign({ screenSharing: true, screenShareFailed: false }),
    clearSharing: assign({ screenSharing: false }),
    setShareFailed: assign({ screenSharing: false, screenShareFailed: true }),
    trackScreenshareStarted: ({ context }) =>
      context.track(
        CAST_EVENTS.screenshareStarted,
        { room_id: context.credentials?.roomId, stub: true },
        context.trackCtx,
      ),
    trackScreenshareFailed: ({ context }) =>
      context.track(
        CAST_EVENTS.screenshareFailed,
        { room_id: context.credentials?.roomId, stub: true },
        context.trackCtx,
      ),
    trackEnding: ({ context }) =>
      context.track(CAST_EVENTS.ending, {}, context.trackCtx),
    trackEnded: ({ context }) =>
      context.track(CAST_EVENTS.ended, { stub: true }, context.trackCtx),
  },
  guards: {
    granted: ({ event }) =>
      "output" in event && (event.output as { granted: boolean }).granted === true,
    published: ({ event }) =>
      "output" in event && (event.output as { published: boolean }).published === true,
  },
}).createMachine({
  id: "castConsole",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    token: input.token,
    identity: input.identity ?? "",
    resolveToken: input.resolveToken ?? simulateResolveToken,
    requestPermissions: input.requestPermissions ?? simulateGrant,
    endCast: input.endCast ?? simulateEndCast,
    shareScreen: input.shareScreen ?? simulateShareScreen,
    track: input.track ?? defaultTrack,
    devices: DEFAULT_DEVICES,
    permissionsDenied: false,
    screenSharing: false,
    screenShareFailed: false,
  }),
  initial: "tokenCheck",
  states: {
    tokenCheck: {
      entry: "trackTokenChecked",
      invoke: {
        id: "resolveToken",
        src: "resolveToken",
        input: ({ context }) => ({
          token: context.token,
          identity: context.identity,
          resolveToken: context.resolveToken,
        }),
        onDone: {
          target: "deviceSelect",
          actions: ["setTokenResult", "trackTokenValid"],
        },
        onError: {
          target: "invalid",
          actions: ["setInvalidReason", "trackInvalidToken"],
        },
      },
    },

    deviceSelect: {
      on: {
        SELECT_DEVICES: {
          target: "permissions",
          actions: ["setDevices", "trackDevicesSelected"],
        },
      },
    },

    permissions: {
      entry: "clearDenied",
      invoke: {
        id: "requestPermissions",
        src: "requestPermissions",
        input: ({ context }) => ({
          devices: context.devices,
          requestPermissions: context.requestPermissions,
        }),
        onDone: [
          {
            guard: "granted",
            target: "preview",
            actions: "trackPermissionsGranted",
          },
          {
            actions: ["setDenied", "trackPermissionsDenied"],
          },
        ],
        onError: {
          actions: ["setDenied", "trackPermissionsDenied"],
        },
      },
      on: {
        RETRY_PERMISSIONS: { target: "permissions", reenter: true },
        BACK: { target: "deviceSelect" },
      },
    },

    preview: {
      entry: "trackPreviewReady",
      on: {
        JOIN: { target: "live", actions: "trackJoinRequested" },
        BACK: { target: "deviceSelect" },
      },
    },

    live: {
      entry: "trackWentLive",
      initial: "idle",
      on: {
        LEAVE: { target: "ending", actions: "trackEnding" },
      },
      states: {
        idle: {
          on: {
            TOGGLE_SCREENSHARE: { target: "publishingShare" },
          },
        },
        publishingShare: {
          invoke: {
            id: "shareScreen",
            src: "shareScreen",
            input: ({ context }) => ({ shareScreen: context.shareScreen }),
            onDone: [
              {
                guard: "published",
                target: "sharing",
                actions: ["setSharing", "trackScreenshareStarted"],
              },
              {
                target: "idle",
                actions: ["setShareFailed", "trackScreenshareFailed"],
              },
            ],
            onError: {
              target: "idle",
              actions: ["setShareFailed", "trackScreenshareFailed"],
            },
          },
        },
        sharing: {
          on: {
            TOGGLE_SCREENSHARE: { target: "idle", actions: "clearSharing" },
          },
        },
      },
    },

    ending: {
      invoke: {
        id: "endCast",
        src: "endCast",
        input: ({ context }) => ({ endCast: context.endCast }),
        onDone: { target: "ended" },
        onError: { target: "ended" },
      },
    },

    ended: {
      entry: "trackEnded",
      type: "final",
    },

    invalid: {
      on: {
        RETRY: { target: "tokenCheck" },
      },
    },
  },
});

export type CastMachine = typeof castMachine;

export function resolveCastSnapshot(args: {
  step: CastStateId;
  trackCtx: TrackContext;
  token: string;
  identity?: string;
  resolveToken?: ResolveTokenFn;
  requestPermissions?: RequestPermissionsFn;
  endCast?: EndCastFn;
  shareScreen?: ShareScreenFn;
  track?: TrackFn;
}) {
  const {
    step,
    trackCtx,
    token,
    identity,
    resolveToken,
    requestPermissions,
    endCast,
    shareScreen,
    track,
  } = args;
  if (step === "tokenCheck") return undefined;

  const context: CastContext = {
    trackCtx,
    token,
    identity: identity ?? "",
    resolveToken: resolveToken ?? simulateResolveToken,
    requestPermissions: requestPermissions ?? simulateGrant,
    endCast: endCast ?? simulateEndCast,
    shareScreen: shareScreen ?? simulateShareScreen,
    track: track ?? defaultTrack,
    devices: DEFAULT_DEVICES,
    permissionsDenied: false,
    screenSharing: false,
    screenShareFailed: false,
    info: SIM_STREAM_INFO,
    credentials: {
      url: "wss://livekit.decentraland.org",
      token: "SIMULATED.preview.stub-livekit-jwt",
      roomId: `scene-${SIM_STREAM_INFO.location}-${slugifyRoom(SIM_STREAM_INFO.placeName)}`,
      identity: identity || "Speaker",
    },
    invalidReason: step === "invalid" ? "Invalid streaming key" : undefined,
  };
  const value = step === "live" ? { live: "idle" } : step;
  return castMachine.resolveState({ value, context });
}
