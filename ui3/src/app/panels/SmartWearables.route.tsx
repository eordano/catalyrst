import { useEffect, useRef, useState } from "react";

import { sendBridge, useBridgeState } from "../../overlay/bridge";
import "./smartwearablespanel.css";

type SmartWearablesPanelProps = {
  floating?: boolean;
  onClose?: () => void;
};

function shortPid(pid: string): string {
  const bare = pid.replace(/^urn:decentraland:entity:/, "").split("?")[0] ?? pid;
  return bare.length > 14 ? `${bare.slice(0, 6)}\u{2026}${bare.slice(-6)}` : bare;
}

export default function SmartWearablesPanel(_props: SmartWearablesPanelProps = {}) {
  const portables = useBridgeState((s) => s.portables);
  // Pending rows survive re-renders but reconcile on the next portables push:
  // a stopped portable drops out of the list, a survivor gets its button back.
  const [stopping, setStopping] = useState<ReadonlySet<string>>(new Set());
  const lastPushRef = useRef(portables);
  useEffect(() => {
    if (lastPushRef.current === portables) return;
    lastPushRef.current = portables;
    setStopping((prev) => (prev.size > 0 ? new Set() : prev));
  }, [portables]);

  const stop = (pid: string) => {
    setStopping((prev) => {
      const next = new Set(prev);
      next.add(pid);
      return next;
    });
    sendBridge("KillPortable", { pid });
  };

  const count = portables.length;
  return (
    <div className="swpanel">
      <h2 className="swpanel__title">Portable experiences</h2>
      {count === 0 ? (
        <>
          <p className="swpanel__empty">Nothing is running right now.</p>
          <p className="swpanel__hint">
            Smart wearables and world apps can run alongside the scene you&#x2019;re in.
            When one asks for a sensitive capability &#x2014; like your wallet or opening
            a link &#x2014; the permission prompt appears automatically.
          </p>
        </>
      ) : (
        <>
          <p className="swpanel__count">
            You have {count} Portable Experience{count === 1 ? "" : "s"} activated
          </p>
          <ul className="swpanel__list">
            {portables.map((p) => (
              <li key={p.pid} className="swpanel__item">
                <span className="swpanel__meta">
                  <span className="swpanel__name">
                    {p.name || p.ens || shortPid(p.pid)}
                  </span>
                  {p.parentCid ? (
                    <span className="swpanel__source">spawned by the scene</span>
                  ) : p.ens ? (
                    <span className="swpanel__source">{p.ens}</span>
                  ) : null}
                </span>
                <button
                  type="button"
                  className="swpanel__stop"
                  disabled={stopping.has(p.pid)}
                  onClick={() => stop(p.pid)}
                >
                  {stopping.has(p.pid) ? "Stopping\u{2026}" : "Stop"}
                </button>
              </li>
            ))}
          </ul>
          <p className="swpanel__hint">
            Deactivate a PEX by unequipping the related Smart Wearable.
          </p>
        </>
      )}
    </div>
  );
}
