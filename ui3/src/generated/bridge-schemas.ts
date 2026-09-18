// GENERATED from catalyrst/ui3/src/generated/bridge by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { AvatarColor3 } from "./bridge/AvatarColor3";
import type { BootstrapPhase } from "./bridge/BootstrapPhase";
import type { BridgeAction } from "./bridge/BridgeAction";
import type { CancelTravelPayload } from "./bridge/CancelTravelPayload";
import type { ChangeRealmPayload } from "./bridge/ChangeRealmPayload";
import type { ClientReadiness } from "./bridge/ClientReadiness";
import type { ConnectionPhase } from "./bridge/ConnectionPhase";
import type { ConnectionStatus } from "./bridge/ConnectionStatus";
import type { Delivery } from "./bridge/Delivery";
import type { FriendEntry } from "./bridge/FriendEntry";
import type { FriendRef } from "./bridge/FriendRef";
import type { FriendRequestEntry } from "./bridge/FriendRequestEntry";
import type { FriendsRequestPayload } from "./bridge/FriendsRequestPayload";
import type { KillPortablePayload } from "./bridge/KillPortablePayload";
import type { LifecycleCommandKind } from "./bridge/LifecycleCommandKind";
import type { LifecycleCommandResult } from "./bridge/LifecycleCommandResult";
import type { LifecycleSnapshot } from "./bridge/LifecycleSnapshot";
import type { NativeHostEvent } from "./bridge/NativeHostEvent";
import type { NativeHostMessage } from "./bridge/NativeHostMessage";
import type { NavigationPhase } from "./bridge/NavigationPhase";
import type { NearbyPlayer } from "./bridge/NearbyPlayer";
import type { Operation } from "./bridge/Operation";
import type { OverlayPush } from "./bridge/OverlayPush";
import type { ParamKind } from "./bridge/ParamKind";
import type { PermissionScope } from "./bridge/PermissionScope";
import type { PlacementPhase } from "./bridge/PlacementPhase";
import type { PlayEmotePayload } from "./bridge/PlayEmotePayload";
import type { PortableEntry } from "./bridge/PortableEntry";
import type { RealmPhase } from "./bridge/RealmPhase";
import type { RealmStatus } from "./bridge/RealmStatus";
import type { ResidencyPhase } from "./bridge/ResidencyPhase";
import type { ResolvePermissionPayload } from "./bridge/ResolvePermissionPayload";
import type { RetryConnectionPayload } from "./bridge/RetryConnectionPayload";
import type { RoomPhase } from "./bridge/RoomPhase";
import type { RotateAvatarPreviewPayload } from "./bridge/RotateAvatarPreviewPayload";
import type { ScenePhase } from "./bridge/ScenePhase";
import type { SceneRoomStatus } from "./bridge/SceneRoomStatus";
import type { SendChatPayload } from "./bridge/SendChatPayload";
import type { ServiceDef } from "./bridge/ServiceDef";
import type { ServiceValue } from "./bridge/ServiceValue";
import type { SetAvatarBasePayload } from "./bridge/SetAvatarBasePayload";
import type { SetAvatarEquipPayload } from "./bridge/SetAvatarEquipPayload";
import type { SetAvatarPayload } from "./bridge/SetAvatarPayload";
import type { SetCameraModePayload } from "./bridge/SetCameraModePayload";
import type { SetExplorerUiOpenPayload } from "./bridge/SetExplorerUiOpenPayload";
import type { SetIdentityPayload } from "./bridge/SetIdentityPayload";
import type { SetMicPayload } from "./bridge/SetMicPayload";
import type { SetSettingPayload } from "./bridge/SetSettingPayload";
import type { SetTimeOfDayPayload } from "./bridge/SetTimeOfDayPayload";
import type { SettingEntry } from "./bridge/SettingEntry";
import type { SettingVariant } from "./bridge/SettingVariant";
import type { SetVoiceParticipantVolumePayload } from "./bridge/SetVoiceParticipantVolumePayload";
import type { SignedFetchPayload } from "./bridge/SignedFetchPayload";
import type { SignRequestPayload } from "./bridge/SignRequestPayload";
import type { TeleportPayload } from "./bridge/TeleportPayload";
import type { TravelOutcome } from "./bridge/TravelOutcome";
import type { TravelPayload } from "./bridge/TravelPayload";
import type { TravelPhase } from "./bridge/TravelPhase";
import type { TravelStatus } from "./bridge/TravelStatus";
import type { VoiceParticipant } from "./bridge/VoiceParticipant";
import type { WebParam } from "./bridge/WebParam";
import type { WorkerPhase } from "./bridge/WorkerPhase";

export const AvatarColor3Schema = z.object({
  r: z.number(),
  g: z.number(),
  b: z.number(),
});

export const BootstrapPhaseSchema = z.enum(["idle", "prerequisites", "initializing", "ready", "starting", "lobby", "enteringWorld", "world", "failed"]);

export const BridgeActionSchema = z.enum(["Travel", "CancelTravel", "RetryConnection", "GetLifecycleSnapshot", "Teleport", "ChangeRealm", "SendChat", "friends.request", "SignRequest", "PlayEmote", "StopEmote", "SetTimeOfDay", "GetSettings", "SetSetting", "SetMic", "SetMicEnabled", "SetAvatar", "SetIdentity", "LoginGuest", "LoginNew", "Logout", "SignedFetch", "RequestAvatarPreview", "RotateAvatarPreview", "ResolvePermission", "CapturePhoto", "SetCameraMode", "SetVoiceParticipantVolume", "SetExplorerUiOpen", "KillPortable"]);

export const CancelTravelPayloadSchema = z.object({
  requestId: z.string(),
  expectedSession: z.string(),
});

export const ChangeRealmPayloadSchema = z.object({
  realm: z.string(),
  position: z.string().optional(),
});

export const PlacementPhaseSchema = z.enum(["waitingIdentity", "waitingRealm", "waitingPointers", "waitingScene", "waitingColliders", "placing", "placed", "degraded", "cancelled"]);

export const ScenePhaseSchema = z.enum(["queued", "entity", "crdt", "javascript", "waitingAdmission", "starting", "running", "empty", "failed", "retired"]);

export const ClientReadinessSchema = z.object({
  placement: PlacementPhaseSchema.nullable(),
  scene: ScenePhaseSchema.nullable(),
  avatarReady: z.boolean(),
  movementReady: z.boolean(),
  pendingAssets: z.number(),
  canExplore: z.boolean(),
  globalRoom: z.boolean().nullable(),
  sceneRoom: z.boolean().nullable(),
});

export const ConnectionPhaseSchema = z.enum(["down", "connecting", "waitingIdentity", "awaitingChallenge", "signing", "awaitingResponse", "established", "dead"]);

export const ConnectionStatusSchema = z.object({
  id: z.string(),
  protocol: z.string(),
  scene: z.string().nullable(),
  control: z.boolean(),
  phase: ConnectionPhaseSchema,
  attempt: z.number(),
  error: z.string().nullable(),
  canRetry: z.boolean(),
});

export const DeliverySchema = z.enum(["launch", "destination", "host", "resolved"]);

export const FriendEntrySchema = z.object({
  address: z.string(),
  name: z.string(),
  hasClaimedName: z.boolean(),
  profilePictureUrl: z.string(),
  status: z.string(),
});

export const FriendRefSchema = z.object({
  address: z.string(),
  name: z.string(),
  hasClaimedName: z.boolean(),
  profilePictureUrl: z.string(),
});

export const FriendRequestEntrySchema = z.object({
  id: z.string(),
  createdAt: z.coerce.bigint(),
  message: z.string().nullable(),
  friend: FriendRefSchema,
});

export const FriendsRequestPayloadSchema = z.object({
  address: z.string(),
});

export const KillPortablePayloadSchema = z.object({
  pid: z.string(),
});

export const LifecycleCommandKindSchema = z.enum(["travel", "cancelTravel", "retryConnection"]);

export const LifecycleCommandResultSchema = z.object({
  requestId: z.string(),
  action: LifecycleCommandKindSchema,
  accepted: z.boolean(),
  error: z.string().nullable(),
});

export const RealmPhaseSchema = z.enum(["idle", "resolving", "fetching", "transitioning", "active", "failed", "cancelled"]);

export const RealmStatusSchema = z.object({
  phase: RealmPhaseSchema,
  generation: z.number(),
  destination: z.string().nullable(),
  error: z.string().nullable(),
});

export const RoomPhaseSchema = z.enum(["detached", "acquiring", "joining", "attached", "reconnecting", "backoff", "blocked", "leaving"]);

export const SceneRoomStatusSchema = z.object({
  phase: RoomPhaseSchema,
  scene: z.string().nullable(),
  error: z.string().nullable(),
});

export const OperationSchema = z.object({
  session: z.string(),
  realm: z.number(),
  instance: z.number(),
  requestId: z.string(),
  attempt: z.number(),
});

export const TravelPhaseSchema = z.enum(["idle", "requested", "resolving", "preparing", "placing", "arrived", "degraded", "failed", "cancelled", "superseded"]);

export const TravelOutcomeSchema = z.object({
  operation: OperationSchema,
  phase: TravelPhaseSchema,
  reason: z.string().nullable(),
});

export const TravelStatusSchema = z.object({
  operation: OperationSchema,
  realmOperation: OperationSchema.nullable(),
  phase: TravelPhaseSchema,
  realm: z.string(),
  parcel: z.tuple([z.number(), z.number()]).nullable(),
  blockingReason: z.string().nullable(),
  canCancel: z.boolean(),
});

export const LifecycleSnapshotSchema = z.object({
  session: z.string(),
  revision: z.number(),
  realm: RealmStatusSchema,
  travel: TravelStatusSchema.nullable(),
  readiness: ClientReadinessSchema,
  outcomes: z.array(TravelOutcomeSchema),
  commandResults: z.array(LifecycleCommandResultSchema),
  connections: z.array(ConnectionStatusSchema),
  sceneRoom: SceneRoomStatusSchema.nullable(),
});

export const NativeHostEventSchema = z.union([z.object({
  t: z.literal("ready"),
  site: z.string(),
  preview: z.boolean(),
  platform: z.string(),
  publicJson: z.string(),
}), z.object({
  t: z.literal("pointerGrab"),
  grabbed: z.boolean(),
}), z.object({
  t: z.literal("consoleReply"),
  id: z.number(),
  ok: z.boolean(),
  body: z.string(),
})]);

export const NativeHostMessageSchema = z.union([z.object({
  t: z.literal("bridge"),
  action: z.string(),
  payload: z.string(),
}), z.object({
  t: z.literal("engineStart"),
  realm: z.string().nullable().optional(),
  parcel: z.tuple([z.number(), z.number()]).nullable().optional(),
}), z.object({
  t: z.literal("pointerRegions"),
  w: z.number(),
  h: z.number(),
  rects: z.array(z.tuple([z.number(), z.number(), z.number(), z.number()])),
}), z.object({
  t: z.literal("keyboardFocus"),
  want: z.boolean(),
}), z.object({
  t: z.literal("openExternal"),
  url: z.string(),
}), z.object({
  t: z.literal("clipboardWrite"),
  text: z.string(),
}), z.object({
  t: z.literal("fullscreen"),
  on: z.boolean(),
}), z.object({
  t: z.literal("console"),
  id: z.number(),
  line: z.string(),
}), z.object({
  t: z.literal("log"),
  level: z.string(),
  msg: z.string(),
})]);

export const NavigationPhaseSchema = z.enum(["idle", "moving", "arrived", "blocked", "cancelled"]);

export const NearbyPlayerSchema = z.object({
  address: z.string(),
  name: z.string(),
  wearables: z.array(z.string()),
  coords: z.string(),
  picture: z.string().optional(),
});

export const PortableEntrySchema = z.object({
  pid: z.string(),
  name: z.string(),
  ens: z.string().optional(),
  parentCid: z.string().optional(),
});

export const SettingVariantSchema = z.object({
  name: z.string(),
  description: z.string(),
});

export const SettingEntrySchema = z.object({
  name: z.string(),
  category: z.string(),
  description: z.string(),
  minValue: z.number(),
  maxValue: z.number(),
  namedVariants: z.array(SettingVariantSchema),
  stepSize: z.number(),
  value: z.number(),
  default: z.number(),
});

export const VoiceParticipantSchema = z.object({
  address: z.string(),
  name: z.string(),
  volume: z.number(),
  speaking: z.boolean(),
});

export const OverlayPushSchema = z.union([z.object({
  kind: z.literal("lifecycle"),
  snapshot: LifecycleSnapshotSchema,
}), z.object({
  kind: z.literal("identity"),
  address: z.string(),
  signerAddress: z.string(),
  isGuest: z.boolean(),
  name: z.string().optional(),
  tag: z.string().optional(),
}), z.object({
  kind: z.literal("avatar"),
  bodyShape: z.string().nullable(),
  wearables: z.array(z.string()),
  emotes: z.array(z.string()),
}), z.object({
  kind: z.literal("scene"),
  title: z.string(),
  coords: z.string(),
  realm: z.string(),
}), z.object({
  kind: z.literal("loading"),
  percent: z.number(),
  ready: z.boolean(),
  avatarLoaded: z.boolean(),
  pendingAssets: z.number().optional(),
}), z.object({
  kind: z.literal("players"),
  players: z.array(NearbyPlayerSchema),
}), z.object({
  kind: z.literal("mic"),
  enabled: z.boolean(),
  available: z.boolean(),
}), z.object({
  kind: z.literal("connection"),
  sceneHealth: z.string(),
  sceneRoom: z.boolean(),
  globalRoom: z.boolean(),
}), z.object({
  kind: z.literal("friends"),
  onlineCount: z.number(),
  friends: z.array(FriendEntrySchema),
  received: z.array(FriendRequestEntrySchema),
  sent: z.array(FriendRequestEntrySchema),
  blocked: z.array(z.string()).optional(),
  blockedByMe: z.array(z.string()).optional(),
}), z.object({
  kind: z.literal("chat"),
  senderName: z.string(),
  senderAddress: z.string(),
  message: z.string(),
  channel: z.string(),
  timestamp: z.number(),
}), z.object({
  kind: z.literal("avatarPreview"),
  dataUrl: z.string(),
}), z.object({
  kind: z.literal("photo"),
  dataUrl: z.string(),
}), z.object({
  kind: z.literal("signedFetchResult"),
  id: z.string(),
  status: z.number(),
  body: z.string(),
}), z.object({
  kind: z.literal("loginCode"),
  code: z.number().optional(),
  url: z.string().optional(),
  error: z.string().optional(),
}), z.object({
  kind: z.literal("permissionRequest"),
  id: z.number(),
  ty: z.string(),
  scene: z.string(),
  sceneName: z.string(),
  additional: z.string().nullable(),
  title: z.string(),
  request: z.string(),
}), z.object({
  kind: z.literal("permissionWithdrawn"),
  id: z.number(),
}), z.object({
  kind: z.literal("playerPosition"),
  position: z.tuple([z.number(), z.number(), z.number()]),
  rotation: z.tuple([z.number(), z.number(), z.number(), z.number()]),
  heading: z.number(),
  parcel: z.string(),
  realm: z.string(),
}), z.object({
  kind: z.literal("toast"),
  key: z.string(),
  message: z.string(),
  shown: z.boolean(),
}), z.object({
  kind: z.literal("voiceParticipants"),
  participants: z.array(VoiceParticipantSchema),
}), z.object({
  kind: z.literal("settings"),
  settings: z.array(SettingEntrySchema),
}), z.object({
  kind: z.literal("portables"),
  portables: z.array(PortableEntrySchema),
}), z.object({
  kind: z.literal("openExplorerUi"),
  ui: z.string(),
  nonce: z.number(),
})]);

export const ParamKindSchema = z.enum(["string", "flag", "bool", "number"]);

export const PermissionScopeSchema = z.enum(["once", "scene", "realm", "global"]);

export const PlayEmotePayloadSchema = z.object({
  urn: z.string(),
});

export const ResidencyPhaseSchema = z.enum(["wanted", "loading", "resident", "pendingEviction", "retiring", "released"]);

export const ResolvePermissionPayloadSchema = z.object({
  id: z.number(),
  allow: z.boolean(),
  level: PermissionScopeSchema.optional(),
});

export const RetryConnectionPayloadSchema = z.object({
  requestId: z.string(),
  expectedSession: z.string(),
  connectionId: z.string(),
});

export const RotateAvatarPreviewPayloadSchema = z.object({
  yaw: z.number(),
});

export const SendChatPayloadSchema = z.object({
  message: z.string(),
  channel: z.string().optional(),
});

export const ServiceValueSchema = z.enum(["http", "websocket", "authority"]);

export const ServiceDefSchema = z.object({
  name: z.string(),
  value: ServiceValueSchema,
  scheme: z.string(),
  sub: z.string(),
  path: z.string(),
});

export const SetAvatarBasePayloadSchema = z.object({
  skinColor: AvatarColor3Schema.optional(),
  eyesColor: AvatarColor3Schema.optional(),
  hairColor: AvatarColor3Schema.optional(),
  bodyShapeUrn: z.string(),
  name: z.string(),
});

export const SetAvatarEquipPayloadSchema = z.object({
  wearableUrns: z.array(z.string()),
  emoteUrns: z.array(z.string()),
  forceRender: z.array(z.string()),
});

export const SetAvatarPayloadSchema = z.object({
  base: SetAvatarBasePayloadSchema.optional(),
  equip: SetAvatarEquipPayloadSchema.optional(),
  has_claimed_name: z.boolean().optional(),
  profile_extras: z.record(z.string(), z.unknown()).optional(),
  name_color: AvatarColor3Schema.nullable().optional(),
});

export const SetCameraModePayloadSchema = z.object({
  detached: z.boolean(),
});

export const SetExplorerUiOpenPayloadSchema = z.object({
  ui: z.string().nullable(),
});

export const SetIdentityPayloadSchema = z.object({
  signer: z.string(),
  ephemeralPrivateKey: z.string(),
  message: z.string(),
  signature: z.string(),
});

export const SetMicPayloadSchema = z.object({
  enabled: z.boolean(),
});

export const SetSettingPayloadSchema = z.object({
  name: z.string(),
  value: z.number(),
});

export const SetTimeOfDayPayloadSchema = z.object({
  minutes: z.number(),
  auto: z.boolean(),
});

export const SetVoiceParticipantVolumePayloadSchema = z.object({
  address: z.string(),
  volume: z.number(),
});

export const SignRequestPayloadSchema = z.object({
  kind: z.string(),
  action: z.string(),
  address: z.string(),
  message: z.string().optional(),
});

export const SignedFetchPayloadSchema = z.object({
  id: z.string(),
  url: z.string(),
  method: z.string().optional(),
  body: z.string().optional(),
});

export const TeleportPayloadSchema = z.object({
  x: z.number(),
  z: z.number(),
});

export const TravelPayloadSchema = z.object({
  requestId: z.string(),
  expectedSession: z.string(),
  realm: z.string().nullable(),
  parcel: z.tuple([z.number(), z.number()]).nullable(),
  spawnPoint: z.string().nullable(),
});

export const WebParamSchema = z.object({
  name: z.string(),
  kind: ParamKindSchema,
  delivery: DeliverySchema,
  doc: z.string(),
});

export const WorkerPhaseSchema = z.enum(["queued", "waitingWorker", "waitingBootstrap", "readyToBind", "binding", "running", "failed", "backoff", "retiring", "released"]);

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertAvatarColor3 = Assert<Mutual<AvatarColor3, z.infer<typeof AvatarColor3Schema>>>;
export type _AssertBootstrapPhase = Assert<Mutual<BootstrapPhase, z.infer<typeof BootstrapPhaseSchema>>>;
export type _AssertBridgeAction = Assert<Mutual<BridgeAction, z.infer<typeof BridgeActionSchema>>>;
export type _AssertCancelTravelPayload = Assert<Mutual<CancelTravelPayload, z.infer<typeof CancelTravelPayloadSchema>>>;
export type _AssertChangeRealmPayload = Assert<Mutual<ChangeRealmPayload, z.infer<typeof ChangeRealmPayloadSchema>>>;
export type _AssertClientReadiness = Assert<Mutual<ClientReadiness, z.infer<typeof ClientReadinessSchema>>>;
export type _AssertConnectionPhase = Assert<Mutual<ConnectionPhase, z.infer<typeof ConnectionPhaseSchema>>>;
export type _AssertConnectionStatus = Assert<Mutual<ConnectionStatus, z.infer<typeof ConnectionStatusSchema>>>;
export type _AssertDelivery = Assert<Mutual<Delivery, z.infer<typeof DeliverySchema>>>;
export type _AssertFriendEntry = Assert<Mutual<FriendEntry, z.infer<typeof FriendEntrySchema>>>;
export type _AssertFriendRef = Assert<Mutual<FriendRef, z.infer<typeof FriendRefSchema>>>;
export type _AssertFriendRequestEntry = Assert<Mutual<FriendRequestEntry, z.infer<typeof FriendRequestEntrySchema>>>;
export type _AssertFriendsRequestPayload = Assert<Mutual<FriendsRequestPayload, z.infer<typeof FriendsRequestPayloadSchema>>>;
export type _AssertKillPortablePayload = Assert<Mutual<KillPortablePayload, z.infer<typeof KillPortablePayloadSchema>>>;
export type _AssertLifecycleCommandKind = Assert<Mutual<LifecycleCommandKind, z.infer<typeof LifecycleCommandKindSchema>>>;
export type _AssertLifecycleCommandResult = Assert<Mutual<LifecycleCommandResult, z.infer<typeof LifecycleCommandResultSchema>>>;
export type _AssertLifecycleSnapshot = Assert<Mutual<LifecycleSnapshot, z.infer<typeof LifecycleSnapshotSchema>>>;
export type _AssertNativeHostEvent = Assert<Mutual<NativeHostEvent, z.infer<typeof NativeHostEventSchema>>>;
export type _AssertNativeHostMessage = Assert<Mutual<NativeHostMessage, z.infer<typeof NativeHostMessageSchema>>>;
export type _AssertNavigationPhase = Assert<Mutual<NavigationPhase, z.infer<typeof NavigationPhaseSchema>>>;
export type _AssertNearbyPlayer = Assert<Mutual<NearbyPlayer, z.infer<typeof NearbyPlayerSchema>>>;
export type _AssertOperation = Assert<Mutual<Operation, z.infer<typeof OperationSchema>>>;
export type _AssertOverlayPush = Assert<Mutual<OverlayPush, z.infer<typeof OverlayPushSchema>>>;
export type _AssertParamKind = Assert<Mutual<ParamKind, z.infer<typeof ParamKindSchema>>>;
export type _AssertPermissionScope = Assert<Mutual<PermissionScope, z.infer<typeof PermissionScopeSchema>>>;
export type _AssertPlacementPhase = Assert<Mutual<PlacementPhase, z.infer<typeof PlacementPhaseSchema>>>;
export type _AssertPlayEmotePayload = Assert<Mutual<PlayEmotePayload, z.infer<typeof PlayEmotePayloadSchema>>>;
export type _AssertPortableEntry = Assert<Mutual<PortableEntry, z.infer<typeof PortableEntrySchema>>>;
export type _AssertRealmPhase = Assert<Mutual<RealmPhase, z.infer<typeof RealmPhaseSchema>>>;
export type _AssertRealmStatus = Assert<Mutual<RealmStatus, z.infer<typeof RealmStatusSchema>>>;
export type _AssertResidencyPhase = Assert<Mutual<ResidencyPhase, z.infer<typeof ResidencyPhaseSchema>>>;
export type _AssertResolvePermissionPayload = Assert<Mutual<ResolvePermissionPayload, z.infer<typeof ResolvePermissionPayloadSchema>>>;
export type _AssertRetryConnectionPayload = Assert<Mutual<RetryConnectionPayload, z.infer<typeof RetryConnectionPayloadSchema>>>;
export type _AssertRoomPhase = Assert<Mutual<RoomPhase, z.infer<typeof RoomPhaseSchema>>>;
export type _AssertRotateAvatarPreviewPayload = Assert<Mutual<RotateAvatarPreviewPayload, z.infer<typeof RotateAvatarPreviewPayloadSchema>>>;
export type _AssertScenePhase = Assert<Mutual<ScenePhase, z.infer<typeof ScenePhaseSchema>>>;
export type _AssertSceneRoomStatus = Assert<Mutual<SceneRoomStatus, z.infer<typeof SceneRoomStatusSchema>>>;
export type _AssertSendChatPayload = Assert<Mutual<SendChatPayload, z.infer<typeof SendChatPayloadSchema>>>;
export type _AssertServiceDef = Assert<Mutual<ServiceDef, z.infer<typeof ServiceDefSchema>>>;
export type _AssertServiceValue = Assert<Mutual<ServiceValue, z.infer<typeof ServiceValueSchema>>>;
export type _AssertSetAvatarBasePayload = Assert<Mutual<SetAvatarBasePayload, z.infer<typeof SetAvatarBasePayloadSchema>>>;
export type _AssertSetAvatarEquipPayload = Assert<Mutual<SetAvatarEquipPayload, z.infer<typeof SetAvatarEquipPayloadSchema>>>;
export type _AssertSetAvatarPayload = Assert<Mutual<SetAvatarPayload, z.infer<typeof SetAvatarPayloadSchema>>>;
export type _AssertSetCameraModePayload = Assert<Mutual<SetCameraModePayload, z.infer<typeof SetCameraModePayloadSchema>>>;
export type _AssertSetExplorerUiOpenPayload = Assert<Mutual<SetExplorerUiOpenPayload, z.infer<typeof SetExplorerUiOpenPayloadSchema>>>;
export type _AssertSetIdentityPayload = Assert<Mutual<SetIdentityPayload, z.infer<typeof SetIdentityPayloadSchema>>>;
export type _AssertSetMicPayload = Assert<Mutual<SetMicPayload, z.infer<typeof SetMicPayloadSchema>>>;
export type _AssertSetSettingPayload = Assert<Mutual<SetSettingPayload, z.infer<typeof SetSettingPayloadSchema>>>;
export type _AssertSetTimeOfDayPayload = Assert<Mutual<SetTimeOfDayPayload, z.infer<typeof SetTimeOfDayPayloadSchema>>>;
export type _AssertSettingEntry = Assert<Mutual<SettingEntry, z.infer<typeof SettingEntrySchema>>>;
export type _AssertSettingVariant = Assert<Mutual<SettingVariant, z.infer<typeof SettingVariantSchema>>>;
export type _AssertSetVoiceParticipantVolumePayload = Assert<Mutual<SetVoiceParticipantVolumePayload, z.infer<typeof SetVoiceParticipantVolumePayloadSchema>>>;
export type _AssertSignedFetchPayload = Assert<Mutual<SignedFetchPayload, z.infer<typeof SignedFetchPayloadSchema>>>;
export type _AssertSignRequestPayload = Assert<Mutual<SignRequestPayload, z.infer<typeof SignRequestPayloadSchema>>>;
export type _AssertTeleportPayload = Assert<Mutual<TeleportPayload, z.infer<typeof TeleportPayloadSchema>>>;
export type _AssertTravelOutcome = Assert<Mutual<TravelOutcome, z.infer<typeof TravelOutcomeSchema>>>;
export type _AssertTravelPayload = Assert<Mutual<TravelPayload, z.infer<typeof TravelPayloadSchema>>>;
export type _AssertTravelPhase = Assert<Mutual<TravelPhase, z.infer<typeof TravelPhaseSchema>>>;
export type _AssertTravelStatus = Assert<Mutual<TravelStatus, z.infer<typeof TravelStatusSchema>>>;
export type _AssertVoiceParticipant = Assert<Mutual<VoiceParticipant, z.infer<typeof VoiceParticipantSchema>>>;
export type _AssertWebParam = Assert<Mutual<WebParam, z.infer<typeof WebParamSchema>>>;
export type _AssertWorkerPhase = Assert<Mutual<WorkerPhase, z.infer<typeof WorkerPhaseSchema>>>;
