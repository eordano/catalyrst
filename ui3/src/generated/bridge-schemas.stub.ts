// GENERATED from src/generated/bridge-schemas.ts by catalyrst/ui3/scripts/gen-schema-stubs.mts. Do not edit.
//
// Performance-mode stand-in: the perf build aliases the real module here so
// zod leaves the bundle. Every export is the same always-accepting shim, so a
// call site that parses its schema directly keeps working and one that hands it
// to `check` never looks at it -- see src/validate/unchecked.ts.
//
// Accepting everything is the trade the mode makes, and it is a real one. The
// transforms go with the schemas, so a nullish field stays undefined instead of
// normalizing to null; and a reader that used validation to DROP a bad row now
// hands that row to its view mapper, which can throw on a field the row does not
// have. Performance mode trusts the wire -- turn it on only where that holds.

const accept = {
  parse: (value: unknown) => value,
  safeParse: (value: unknown) => ({ success: true as const, data: value }),
} as never;

export const AvatarColor3Schema = accept;
export const BridgeActionSchema = accept;
export const ChangeRealmPayloadSchema = accept;
export const FriendEntrySchema = accept;
export const FriendRefSchema = accept;
export const FriendRequestEntrySchema = accept;
export const FriendsRequestPayloadSchema = accept;
export const KillPortablePayloadSchema = accept;
export const NativeHostEventSchema = accept;
export const NativeHostMessageSchema = accept;
export const NearbyPlayerSchema = accept;
export const PortableEntrySchema = accept;
export const SettingVariantSchema = accept;
export const SettingEntrySchema = accept;
export const VoiceParticipantSchema = accept;
export const OverlayPushSchema = accept;
export const PermissionScopeSchema = accept;
export const PlayEmotePayloadSchema = accept;
export const ResolvePermissionPayloadSchema = accept;
export const RotateAvatarPreviewPayloadSchema = accept;
export const SendChatPayloadSchema = accept;
export const SetAvatarBasePayloadSchema = accept;
export const SetAvatarEquipPayloadSchema = accept;
export const SetAvatarPayloadSchema = accept;
export const SetCameraModePayloadSchema = accept;
export const SetExplorerUiOpenPayloadSchema = accept;
export const SetIdentityPayloadSchema = accept;
export const SetMicPayloadSchema = accept;
export const SetSettingPayloadSchema = accept;
export const SetTimeOfDayPayloadSchema = accept;
export const SetVoiceParticipantVolumePayloadSchema = accept;
export const SignRequestPayloadSchema = accept;
export const SignedFetchPayloadSchema = accept;
export const TeleportPayloadSchema = accept;
