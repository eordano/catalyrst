export {
  getBridge,
  sendBridge,
  subscribeBridge,
  getDeployIdentity,
  emoteUrnForName,
} from "@ui/overlay/bridge";

export type { OverlayPush } from "@ui/generated/bridge/OverlayPush";
export type { BridgeAction } from "@ui/generated/bridge/BridgeAction";
export type { FriendEntry } from "@ui/generated/bridge/FriendEntry";
export type { FriendRequestEntry } from "@ui/generated/bridge/FriendRequestEntry";
export type { NearbyPlayer } from "@ui/generated/bridge/NearbyPlayer";
export type { TeleportPayload } from "@ui/generated/bridge/TeleportPayload";
export type { SendChatPayload } from "@ui/generated/bridge/SendChatPayload";
export type { PlayEmotePayload } from "@ui/generated/bridge/PlayEmotePayload";
export type { SetMicPayload } from "@ui/generated/bridge/SetMicPayload";
export type { SetIdentityPayload } from "@ui/generated/bridge/SetIdentityPayload";
export type { SignedFetchPayload } from "@ui/generated/bridge/SignedFetchPayload";
export type { ResolvePermissionPayload } from "@ui/generated/bridge/ResolvePermissionPayload";
