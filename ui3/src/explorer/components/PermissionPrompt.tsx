import { useEffect, useState } from "react";
import { subscribeBridge, sendBridge } from "../../overlay/bridge";
import PermissionDialog, {
  type PermissionDialogRequest,
  type PermissionLevelChoice,
} from "./PermissionDialog";

type PermissionPush = {
  kind: "permissionRequest";
  id?: number;
  ty?: string;
  scene?: string;
  sceneName?: string;
  additional?: string | null;
};

function isPermissionPush(v: unknown): v is PermissionPush {
  return (
    typeof v === "object" &&
    v !== null &&
    (v as { kind?: unknown }).kind === "permissionRequest" &&
    typeof (v as { id?: unknown }).id === "number"
  );
}

export default function PermissionPrompt() {
  const [queue, setQueue] = useState<PermissionDialogRequest[]>([]);

  useEffect(() => {
    return subscribeBridge((push) => {
      if (!isPermissionPush(push)) return;
      const req: PermissionDialogRequest = {
        id: push.id as number,
        ty: push.ty ?? "",
        scene: push.sceneName || push.scene || "",
        additional: push.additional ?? null,
      };
      setQueue((q) => (q.some((x) => x.id === req.id) ? q : [...q, req]));
    });
  }, []);

  const cur = queue[0];
  if (!cur) return null;

  const dequeue = () => setQueue((q) => q.filter((x) => x.id !== cur.id));

  const resolve = (allow: boolean, level: PermissionLevelChoice) => {
    sendBridge("ResolvePermission", { id: cur.id, allow, level });
    dequeue();
  };

  return <PermissionDialog request={cur} onResolve={resolve} />;
}
