import { sendBridge } from "../../overlay/bridge";

export const FRIEND_ACTIONS = Object.freeze({
  REQUEST: "request",
  ACCEPT: "accept",
  CANCEL: "cancel",
  REJECT: "reject",
  DELETE: "delete",
  BLOCK: "block",
  UNBLOCK: "unblock",
});

export function requestFriendAction(
  action: string,
  address: string,
  extra: { message?: string } = {},
): boolean {
  const valid = (Object.values(FRIEND_ACTIONS) as string[]).includes(action);
  if (!valid) {
    if (typeof console !== "undefined") {
      console.warn(`[friends] ignored unknown action: ${action}`);
    }
    return false;
  }
  sendBridge("SignRequest", {
    kind: "upsert_friendship",
    action,
    address,
    ...extra,
  });
  return true;
}
