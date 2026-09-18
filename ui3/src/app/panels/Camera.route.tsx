import { useNavigate } from "react-router";

import { useBridgeState } from "../../overlay/bridge";
import Camera from "../../explorer/pages/Camera";
export { prefetchReel as prefetch } from "../../data/catalyst/reel";

export default function CameraPanel() {
  const navigate = useNavigate();
  const address = useBridgeState((state) => state.identity.address);
  return <Camera key={address ?? "guest"} onClose={() => navigate("/")} />;
}
