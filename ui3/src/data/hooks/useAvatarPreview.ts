import { useEffect } from "react";
import { useBridgeState, sendBridge } from "../../overlay/bridge";

export function useAvatarPreview() {
  const avatarPreview = useBridgeState((s) => s.avatarPreview);
  useEffect(() => {
    sendBridge("RequestAvatarPreview", {});
  }, []);
  return avatarPreview;
}
