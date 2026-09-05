import { useSyncExternalStore } from "react";

import type { AvatarColor3 } from "../generated/bridge/AvatarColor3";
import type { BridgeAction } from "../generated/bridge/BridgeAction";
import type { ChangeRealmPayload } from "../generated/bridge/ChangeRealmPayload";
import type { OverlayPush } from "../generated/bridge/OverlayPush";
import { OverlayPushSchema } from "../generated/bridge-schemas";
import { checkOk } from "../validate";
import type { FriendEntry } from "../generated/bridge/FriendEntry";
import type { KillPortablePayload } from "../generated/bridge/KillPortablePayload";
import type { NearbyPlayer } from "../generated/bridge/NearbyPlayer";
import type { PortableEntry } from "../generated/bridge/PortableEntry";
import type { SetExplorerUiOpenPayload } from "../generated/bridge/SetExplorerUiOpenPayload";
import type { SetAvatarPayload } from "../generated/bridge/SetAvatarPayload";
import type { FriendsRequestPayload } from "../generated/bridge/FriendsRequestPayload";
import type { PlayEmotePayload } from "../generated/bridge/PlayEmotePayload";
import type { ResolvePermissionPayload } from "../generated/bridge/ResolvePermissionPayload";
import type { RotateAvatarPreviewPayload } from "../generated/bridge/RotateAvatarPreviewPayload";
import type { SendChatPayload } from "../generated/bridge/SendChatPayload";
import type { SetCameraModePayload } from "../generated/bridge/SetCameraModePayload";
import type { SetIdentityPayload } from "../generated/bridge/SetIdentityPayload";
import type { SetMicPayload } from "../generated/bridge/SetMicPayload";
import type { SetSettingPayload } from "../generated/bridge/SetSettingPayload";
import type { SetTimeOfDayPayload } from "../generated/bridge/SetTimeOfDayPayload";
import type { SetVoiceParticipantVolumePayload } from "../generated/bridge/SetVoiceParticipantVolumePayload";
import type { SignedFetchPayload } from "../generated/bridge/SignedFetchPayload";
import type { SignRequestPayload } from "../generated/bridge/SignRequestPayload";
import type { TeleportPayload } from "../generated/bridge/TeleportPayload";

export type BridgeApi = {
  send: (action: string, payload?: unknown) => void;
  onState: (cb: (push: unknown) => void) => () => void;
};

export type { AvatarColor3, SetAvatarPayload };

type EmptyPayload = Record<string, never>;

type BridgePayloadMap = {
  Teleport: TeleportPayload;
  ChangeRealm: ChangeRealmPayload;
  SendChat: SendChatPayload;
  "friends.request": FriendsRequestPayload;
  SignRequest: SignRequestPayload;
  PlayEmote: PlayEmotePayload;
  StopEmote: EmptyPayload;
  SetTimeOfDay: SetTimeOfDayPayload;
  GetSettings: EmptyPayload;
  SetSetting: SetSettingPayload;
  SetMic: SetMicPayload;
  SetMicEnabled: SetMicPayload;
  SetAvatar: SetAvatarPayload;
  SetIdentity: SetIdentityPayload;
  LoginGuest: EmptyPayload;
  LoginNew: EmptyPayload;
  Logout: EmptyPayload;
  SignedFetch: SignedFetchPayload;
  RequestAvatarPreview: EmptyPayload;
  RotateAvatarPreview: RotateAvatarPreviewPayload;
  ResolvePermission: ResolvePermissionPayload;
  CapturePhoto: EmptyPayload;
  SetCameraMode: SetCameraModePayload;
  SetVoiceParticipantVolume: SetVoiceParticipantVolumePayload;
  SetExplorerUiOpen: SetExplorerUiOpenPayload;
  KillPortable: KillPortablePayload;
};

export type BridgePayloads = { [K in BridgeAction]: BridgePayloadMap[K] };

type EmptyPayloadAction = {
  [K in BridgeAction]: BridgePayloads[K] extends EmptyPayload ? K : never;
}[BridgeAction];

export type DeployIdentity = {
  name?: string;
  tag?: string | null;
  address?: string;
  wallet?: string;
  isGuest?: boolean;
  signerAddress?: string;
};

export function getBridge(): BridgeApi | null {
  if (typeof window === "undefined") return null;
  return window.dclBridge ?? null;
}

export function sendBridge<K extends EmptyPayloadAction>(
  action: K,
  payload?: BridgePayloads[K],
): void;
export function sendBridge<K extends BridgeAction>(
  action: K,
  payload: BridgePayloads[K],
): void;
export function sendBridge(action: BridgeAction, payload?: unknown): void {
  const bridge = getBridge();
  if (!bridge) return;
  try {
    bridge.send(action, payload);
  } catch {
  }
}

export function subscribeBridge(cb: (push: unknown) => void): () => void {
  const bridge = getBridge();
  if (!bridge) return () => {};
  try {
    return bridge.onState(cb);
  } catch {
    return () => {};
  }
}

// The engine's bridge global lands after the overlay mounts, so subscribers
// retry every 250ms and give up after 10s. One copy of that loop lives here.
export function attachBridge(
  onPush: (push: unknown) => void,
  onAttached?: () => void,
): () => void {
  let unsub: (() => void) | null = null;
  const attach = () => {
    if (unsub || !getBridge()) return false;
    unsub = subscribeBridge(onPush);
    onAttached?.();
    return true;
  };
  if (attach()) return () => unsub?.();
  const iv = setInterval(() => {
    if (attach()) clearInterval(iv);
  }, 250);
  const stop = setTimeout(() => clearInterval(iv), 10000);
  return () => {
    clearInterval(iv);
    clearTimeout(stop);
    unsub?.();
  };
}

export function getDeployIdentity(): DeployIdentity | null {
  if (typeof window === "undefined") return null;
  const id = window.dclDeployIdentity;
  if (!id || id.isGuest || !id.signerAddress) return null;
  return id;
}

const BASE_EMOTE_URNS: Record<string, string> = {
  wave: "urn:decentraland:off-chain:base-emotes:wave",
  clap: "urn:decentraland:off-chain:base-emotes:clap",
  dance: "urn:decentraland:off-chain:base-emotes:dance",
  kiss: "urn:decentraland:off-chain:base-emotes:kiss",
  headexplode: "urn:decentraland:off-chain:base-emotes:headexplode",
  robot: "urn:decentraland:off-chain:base-emotes:robot",
  hammer: "urn:decentraland:off-chain:base-emotes:hammer",
  tik: "urn:decentraland:off-chain:base-emotes:tik",
  snowfall: "urn:decentraland:off-chain:base-emotes:snowfall",
  disco: "urn:decentraland:off-chain:base-emotes:disco",
};

export function emoteUrnForName(name: string): string | null {
  const raw = name.trim();
  if (raw.startsWith("urn:")) return raw;
  return BASE_EMOTE_URNS[raw.toLowerCase()] ?? null;
}

export function stopEmote(): void {
  sendBridge("StopEmote", {});
}

export type BridgeIdentity = {
  name: string;
  tag: string | null;
  address: string | null;
  wallet: string | null;
  isGuest: boolean;
};

export type BridgeScene = {
  title: string | null;
  coords: string | null;
  realm: string | null;
};

export type BridgeChatLine = {
  senderName?: string;
  senderAddress?: string;
  message?: string;
  channel?: string;
  timestamp?: number;
};

export type BridgeFriend = FriendEntry;

export type BridgeFriends = {
  onlineCount: number;
  friends: BridgeFriend[];
  blocked: string[];
};

export type BridgeMic = {
  enabled: boolean;
  available: boolean;
};

export type BridgeConnection = {
  sceneHealth: "ok" | "error" | "loading";
  sceneRoom: boolean;
  globalRoom: boolean;
};

export type BridgeLoginCode = {
  code: number | null;
  url: string | null;
  error: string | null;
};

export type BridgeAvatarLoadout = {
  bodyShape: string | null;
  wearables: string[];
  emotes: string[];
};

export type BridgePlayerPosition = {
  position: [number, number, number];
  heading: number;
  parcel: string | null;
  realm: string | null;
};

export type BridgeToast = {
  key: string;
  message: string;
};

export type BridgeLoading = {
  percent: number;
  ready: boolean;
  avatarLoaded: boolean;
  pendingAssets: number;
};

export type BridgeOpenPanel = {
  ui: string;
  nonce: number;
};

export type BridgePortable = PortableEntry;

export type BridgeState = {
  identity: BridgeIdentity;
  scene: BridgeScene;
  chat: BridgeChatLine[];
  players: NearbyPlayer[];
  friends: BridgeFriends;
  mic: BridgeMic;
  connection: BridgeConnection | null;
  loginCode: BridgeLoginCode | null;
  avatarPreview: string | null;
  avatarLoadout: BridgeAvatarLoadout | null;
  playerPosition: BridgePlayerPosition | null;
  toasts: BridgeToast[];
  loading: BridgeLoading | null;
  portables: BridgePortable[];
  openPanel: BridgeOpenPanel | null;
};

export const FALLBACK_STATE: BridgeState = {
  identity: {
    name: "Guest",
    tag: null,
    address: null,
    wallet: null,
    isGuest: true,
  },
  scene: {
    title: null,
    coords: null,
    realm: null,
  },
  chat: [],
  players: [],
  friends: {
    onlineCount: 0,
    friends: [],
    blocked: [],
  },
  mic: {
    enabled: false,
    available: true,
  },
  connection: null,
  loginCode: null,
  avatarPreview: null,
  avatarLoadout: null,
  playerPosition: null,
  toasts: [],
  loading: null,
  portables: [],
  openPanel: null,
};

type BridgePush = OverlayPush;

function isBridgePush(v: unknown): v is BridgePush {
  return (
    typeof v === "object" &&
    v !== null &&
    typeof (v as { kind?: unknown }).kind === "string"
  );
}

// Exported for the applied-check test: the reducer is otherwise reachable only
// through a live engine, and a check nothing calls is the failure mode this
// exists to catch.
export function applyBridgePushForTest(push: unknown): BridgeState {
  return applyState(FALLBACK_STATE, push);
}

function applyState(prev: BridgeState, push: unknown): BridgeState {
  if (!isBridgePush(push)) return prev;
  // The engine is a separate build in another language, so its pushes are the
  // least trustworthy input this app has -- and the `?? prev.x` reads below mean
  // a renamed or wrong-typed field does not fail, it silently keeps the old
  // value. Checking here turns that into a dev error and a production report,
  // without changing what the reducer does: `check` hands the value back on a
  // production rejection, so the fallbacks still run exactly as before.
  // Skip, not carry on: the reducer below reads fields off `push` directly
  // (`push.address.slice(...)`), so a wrong-typed field would be a TypeError
  // one line after the rejection was reported. The `?? prev.x` reads only ever
  // guarded ABSENT fields. Returning prev drops the malformed push and leaves
  // the UI on its last good state, which is what those fallbacks intended.
  if (!checkOk(OverlayPushSchema, push, "bridge/push")) return prev;
  switch (push.kind) {
    case "identity": {
      const isGuest = push.isGuest ?? prev.identity.isGuest;
      return {
        ...prev,
        identity: {
          ...prev.identity,
          name: push.name ?? prev.identity.name,
          tag: push.tag ?? prev.identity.tag,
          address: push.address ?? prev.identity.address,
          wallet: push.address
            ? `${push.address.slice(0, 5)}\u{2026}${push.address.slice(-4)}`
            : prev.identity.wallet,
          isGuest,
        },
        loginCode: isGuest ? prev.loginCode : null,
      };
    }
    case "scene":
      return {
        ...prev,
        scene: {
          title: push.title ?? prev.scene.title,
          coords: push.coords ?? prev.scene.coords,
          realm: push.realm ?? prev.scene.realm,
        },
      };
    case "chat": {
      const line: BridgeChatLine = {
        senderName: push.senderName,
        senderAddress: push.senderAddress,
        message: push.message,
        channel: push.channel,
        timestamp: push.timestamp,
      };
      return { ...prev, chat: [...prev.chat.slice(-49), line] };
    }
    case "players":
      return { ...prev, players: push.players ?? prev.players };
    case "friends":
      return {
        ...prev,
        friends: {
          onlineCount: push.onlineCount ?? prev.friends.onlineCount,
          friends: push.friends ?? prev.friends.friends,
          blocked: push.blocked ?? prev.friends.blocked,
        },
      };
    case "mic":
      return {
        ...prev,
        mic: {
          enabled: push.enabled ?? prev.mic.enabled,
          available: push.available ?? prev.mic.available,
        },
      };
    case "connection":
      return {
        ...prev,
        connection: {
          sceneHealth: push.sceneHealth as BridgeConnection["sceneHealth"],
          sceneRoom: push.sceneRoom,
          globalRoom: push.globalRoom,
        },
      };
    case "loginCode":
      return {
        ...prev,
        loginCode: {
          code: push.code != null && push.code !== -1 ? push.code : null,
          url: push.url ?? null,
          error: push.error ?? null,
        },
      };
    case "avatarPreview":
      return { ...prev, avatarPreview: push.dataUrl ?? prev.avatarPreview };
    case "avatar":
      return {
        ...prev,
        avatarLoadout: {
          bodyShape: push.bodyShape ?? prev.avatarLoadout?.bodyShape ?? null,
          wearables: push.wearables ?? prev.avatarLoadout?.wearables ?? [],
          emotes: push.emotes ?? prev.avatarLoadout?.emotes ?? [],
        },
      };
    case "playerPosition":
      return {
        ...prev,
        playerPosition: {
          position: push.position,
          heading: push.heading,
          parcel: push.parcel ?? null,
          realm: push.realm ?? null,
        },
      };
    case "toast": {
      const rest = prev.toasts.filter((t) => t.key !== push.key);
      if (!push.shown) return { ...prev, toasts: rest };
      return {
        ...prev,
        toasts: [...rest.slice(-4), { key: push.key, message: push.message }],
      };
    }
    case "loading":
      return {
        ...prev,
        loading: {
          percent: push.percent,
          ready: push.ready,
          avatarLoaded: push.avatarLoaded,
          pendingAssets: typeof push.pendingAssets === "number" ? push.pendingAssets : 0,
        },
      };
    case "portables":
      return { ...prev, portables: push.portables ?? prev.portables };
    case "openExplorerUi":
      return { ...prev, openPanel: { ui: push.ui, nonce: push.nonce } };
    default:
      return prev;
  }
}

export type BridgeSnapshot = BridgeState & { live: boolean };

const OFFLINE_SNAPSHOT: BridgeSnapshot = { ...FALLBACK_STATE, live: false };

let snapshot: BridgeSnapshot = OFFLINE_SNAPSHOT;
const listeners = new Set<() => void>();
let attached = false;
let attachedBridge: BridgeApi | null = null;
let unsubBridge: (() => void) | null = null;
let polling: ReturnType<typeof setInterval> | null = null;
let resetTimer: ReturnType<typeof setTimeout> | null = null;

function emit(): void {
  for (const l of listeners) l();
}

function tryAttach(): boolean {
  if (attached) return true;
  const bridge = getBridge();
  if (!bridge) return false;
  attached = true;
  attachedBridge = bridge;
  unsubBridge = subscribeBridge((push) => {
    const next = applyState(snapshot, push);
    if (next === snapshot && snapshot.live) return;
    snapshot = { ...next, live: true };
    emit();
  });
  snapshot = { ...snapshot, live: true };
  emit();
  return true;
}

function resetStore(): void {
  try {
    unsubBridge?.();
  } catch {
  }
  unsubBridge = null;
  attached = false;
  attachedBridge = null;
  if (polling) {
    clearInterval(polling);
    polling = null;
  }
  snapshot = OFFLINE_SNAPSHOT;
}

function ensureAttached(): void {
  if (attached && attachedBridge !== getBridge()) resetStore();
  if (attached || tryAttach()) return;
  if (polling) return;
  const iv = setInterval(() => {
    if (tryAttach() && polling === iv) {
      clearInterval(iv);
      polling = null;
    }
  }, 250);
  polling = iv;
  setTimeout(() => {
    if (polling === iv) {
      clearInterval(iv);
      polling = null;
    }
  }, 10000);
}

// Teardown is deferred a tick and cancelled by any resubscribe, so a
// StrictMode unmount/remount pair (or a route swap between two bridge-backed
// panels) keeps the live subscription instead of dropping to the offline
// snapshot and re-attaching.
function subscribeStore(cb: () => void): () => void {
  if (resetTimer !== null) {
    clearTimeout(resetTimer);
    resetTimer = null;
  }
  ensureAttached();
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
    if (listeners.size !== 0 || resetTimer !== null) return;
    resetTimer = setTimeout(() => {
      resetTimer = null;
      if (listeners.size === 0) resetStore();
    }, 0);
  };
}

export function useBridgeState(): BridgeSnapshot;
export function useBridgeState<T>(selector: (s: BridgeSnapshot) => T): T;
export function useBridgeState<T>(
  selector?: (s: BridgeSnapshot) => T,
): T | BridgeSnapshot {
  return useSyncExternalStore(
    subscribeStore,
    () => (selector ? selector(snapshot) : snapshot),
    () => (selector ? selector(OFFLINE_SNAPSHOT) : OFFLINE_SNAPSHOT),
  );
}
