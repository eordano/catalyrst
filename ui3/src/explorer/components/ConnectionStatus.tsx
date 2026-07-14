import type { BridgeConnection } from "../../overlay/bridge";
import "./connectionstatus.css";

type StatusKind = "ok" | "info" | "warn";

type StatusRow = {
  title: string;
  subtitle: string;
  status: StatusKind;
  label: string;
};

type ConnectionStatusProps = {
  connection?: BridgeConnection | null;
  realm?: string | null;
  onClose?: () => void;
};

const SCENE_HEALTH: Record<
  BridgeConnection["sceneHealth"],
  { status: StatusKind; label: string }
> = {
  ok: { status: "ok", label: "Healthy" },
  error: { status: "warn", label: "Errors" },
  loading: { status: "info", label: "Loading" },
};

function rowsFor(
  connection: BridgeConnection | null | undefined,
  realm: string | null | undefined,
): StatusRow[] {
  const scene = connection ? SCENE_HEALTH[connection.sceneHealth] : null;
  return [
    {
      title: "Scene",
      subtitle: "Scene running with or without errors",
      status: scene?.status ?? "info",
      label: scene?.label ?? "…",
    },
    {
      title: "Scene Room",
      subtitle: "Comms room for this scene",
      status: connection?.sceneRoom ? "ok" : "info",
      label: connection == null ? "…" : connection.sceneRoom ? "Connected" : "None",
    },
    {
      title: "Global Room",
      subtitle: "Comms connection to the realm",
      status: connection?.globalRoom ? "ok" : "warn",
      label:
        connection == null ? "…" : connection.globalRoom ? "Connected" : "Disconnected",
    },
    {
      title: "Realm",
      subtitle: "Connected realm",
      status: realm ? "ok" : "info",
      label: realm || "…",
    },
  ];
}

export default function ConnectionStatus({
  connection,
  realm,
  onClose,
}: ConnectionStatusProps) {
  const rows = rowsFor(connection, realm);
  return (
    <div className="xcs__stage">
      <div className="xcs" role="dialog" aria-label="Connection status">
        <div className="xcs__header">
          <span className="xcs__title">CONNECTION STATUS</span>
          <button className="xcs__close" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>
        {rows.map((r, i) => (
          <div className="xcs__row" key={i}>
            <div className="xcs__info">
              <div className="xcs__rowtitle">{r.title}</div>
              <div className="xcs__subtitle">{r.subtitle}</div>
            </div>
            <span className={"xcs__pill xcs__pill--" + r.status}>
              <span className="xcs__dot" /> {r.label}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
