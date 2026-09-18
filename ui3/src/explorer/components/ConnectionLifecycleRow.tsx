import type { ConnectionStatus } from "../../generated/bridge/ConnectionStatus";
import type { ConnectionPhase } from "../../generated/bridge/ConnectionPhase";
import { useLifecycleCommand } from "../../overlay/useLifecycleCommand";

const PHASE_LABEL: Record<ConnectionPhase, string> = {
  down: "Disconnected", connecting: "Connecting", waitingIdentity: "Waiting for identity",
  awaitingChallenge: "Authenticating", signing: "Authenticating", awaitingResponse: "Authenticating",
  established: "Connected", dead: "Closed",
};

export default function ConnectionLifecycleRow({ connection, session }: { connection: ConnectionStatus; session: string }) {
  const retry = useLifecycleCommand();
  const title = connection.control ? "Realm coordination" : connection.scene ? "Scene multiplayer" : "Nearby players";
  const tone = connection.phase === "established" ? "ok" : connection.phase === "down" || connection.phase === "dead" ? "warn" : "info";
  return (
    <div className="xcs__row xcs__transport">
      <div className="xcs__info">
        <div className="xcs__rowtitle">{title}</div>
        <div className="xcs__subtitle">{connection.protocol}{" \u00b7 "}{PHASE_LABEL[connection.phase]}</div>
        {connection.error && <div className="xcs__subtitle">{connection.error}</div>}
        {retry.error && <div className="xcs__subtitle" role="alert">{retry.error}</div>}
      </div>
      {connection.canRetry ? <button className="xcs__retry" type="button" disabled={retry.pending} aria-label={`Retry ${title.toLowerCase()}`} onClick={() => retry.run({
        action: "RetryConnection", payload: { requestId: crypto.randomUUID(), expectedSession: session, connectionId: connection.id },
      })}>{retry.pending ? "Requesting\u2026" : "Retry"}</button> : <span className={`xcs__pill xcs__pill--${tone}`}>{PHASE_LABEL[connection.phase]}</span>}
    </div>
  );
}
