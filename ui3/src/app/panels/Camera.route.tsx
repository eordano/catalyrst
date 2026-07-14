import { useNavigate } from "react-router";

import Camera from "../../explorer/pages/Camera";

export default function CameraPanel() {
  const navigate = useNavigate();
  return <Camera onClose={() => navigate("/")} />;
}
