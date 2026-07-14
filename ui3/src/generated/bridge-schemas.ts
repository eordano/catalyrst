// GENERATED from catalyrst/ui3/src/generated/bridge by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { AvatarColor3 } from "./bridge/AvatarColor3";
import type { BridgeAction } from "./bridge/BridgeAction";
import type { ChangeRealmPayload } from "./bridge/ChangeRealmPayload";
import type { FriendEntry } from "./bridge/FriendEntry";
import type { FriendRef } from "./bridge/FriendRef";
import type { FriendRequestEntry } from "./bridge/FriendRequestEntry";
import type { FriendsRequestPayload } from "./bridge/FriendsRequestPayload";
import type { KillPortablePayload } from "./bridge/KillPortablePayload";
import type { NativeHostEvent } from "./bridge/NativeHostEvent";
import type { NativeHostMessage } from "./bridge/NativeHostMessage";
import type { NearbyPlayer } from "./bridge/NearbyPlayer";
import type { OverlayPush } from "./bridge/OverlayPush";
import type { PermissionScope } from "./bridge/PermissionScope";
import type { PlayEmotePayload } from "./bridge/PlayEmotePayload";
import type { PortableEntry } from "./bridge/PortableEntry";
import type { ResolvePermissionPayload } from "./bridge/ResolvePermissionPayload";
import type { RotateAvatarPreviewPayload } from "./bridge/RotateAvatarPreviewPayload";
import type { SendChatPayload } from "./bridge/SendChatPayload";
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
import type { VoiceParticipant } from "./bridge/VoiceParticipant";

export const AvatarColor3Schema = z.object({
  r: z.number(),
  g: z.number(),
  b: z.number(),
});

export const BridgeActionSchema = z.enum(["Teleport", "ChangeRealm", "SendChat", "friends.request", "SignRequest", "PlayEmote", "StopEmote", "SetTimeOfDay", "GetSettings", "SetSetting", "SetMic", "SetMicEnabled", "SetAvatar", "SetIdentity", "LoginGuest", "LoginNew", "Logout", "SignedFetch", "RequestAvatarPreview", "RotateAvatarPreview", "ResolvePermission", "CapturePhoto", "SetCameraMode", "SetVoiceParticipantVolume", "SetExplorerUiOpen", "KillPortable"]);

export const ChangeRealmPayloadSchema = z.object({
  realm: z.string(),
  position: z.string().optional(),
});

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

export const PermissionScopeSchema = z.enum(["once", "scene", "realm", "global"]);

export const PlayEmotePayloadSchema = z.object({
  urn: z.string(),
});

export const ResolvePermissionPayloadSchema = z.object({
  id: z.number(),
  allow: z.boolean(),
  level: PermissionScopeSchema.optional(),
});

export const RotateAvatarPreviewPayloadSchema = z.object({
  yaw: z.number(),
});

export const SendChatPayloadSchema = z.object({
  message: z.string(),
  channel: z.string().optional(),
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

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertAvatarColor3 = Assert<Mutual<AvatarColor3, z.infer<typeof AvatarColor3Schema>>>;
export type _AssertBridgeAction = Assert<Mutual<BridgeAction, z.infer<typeof BridgeActionSchema>>>;
export type _AssertChangeRealmPayload = Assert<Mutual<ChangeRealmPayload, z.infer<typeof ChangeRealmPayloadSchema>>>;
export type _AssertFriendEntry = Assert<Mutual<FriendEntry, z.infer<typeof FriendEntrySchema>>>;
export type _AssertFriendRef = Assert<Mutual<FriendRef, z.infer<typeof FriendRefSchema>>>;
export type _AssertFriendRequestEntry = Assert<Mutual<FriendRequestEntry, z.infer<typeof FriendRequestEntrySchema>>>;
export type _AssertFriendsRequestPayload = Assert<Mutual<FriendsRequestPayload, z.infer<typeof FriendsRequestPayloadSchema>>>;
export type _AssertKillPortablePayload = Assert<Mutual<KillPortablePayload, z.infer<typeof KillPortablePayloadSchema>>>;
export type _AssertNativeHostEvent = Assert<Mutual<NativeHostEvent, z.infer<typeof NativeHostEventSchema>>>;
export type _AssertNativeHostMessage = Assert<Mutual<NativeHostMessage, z.infer<typeof NativeHostMessageSchema>>>;
export type _AssertNearbyPlayer = Assert<Mutual<NearbyPlayer, z.infer<typeof NearbyPlayerSchema>>>;
export type _AssertOverlayPush = Assert<Mutual<OverlayPush, z.infer<typeof OverlayPushSchema>>>;
export type _AssertPermissionScope = Assert<Mutual<PermissionScope, z.infer<typeof PermissionScopeSchema>>>;
export type _AssertPlayEmotePayload = Assert<Mutual<PlayEmotePayload, z.infer<typeof PlayEmotePayloadSchema>>>;
export type _AssertPortableEntry = Assert<Mutual<PortableEntry, z.infer<typeof PortableEntrySchema>>>;
export type _AssertResolvePermissionPayload = Assert<Mutual<ResolvePermissionPayload, z.infer<typeof ResolvePermissionPayloadSchema>>>;
export type _AssertRotateAvatarPreviewPayload = Assert<Mutual<RotateAvatarPreviewPayload, z.infer<typeof RotateAvatarPreviewPayloadSchema>>>;
export type _AssertSendChatPayload = Assert<Mutual<SendChatPayload, z.infer<typeof SendChatPayloadSchema>>>;
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
export type _AssertVoiceParticipant = Assert<Mutual<VoiceParticipant, z.infer<typeof VoiceParticipantSchema>>>;
