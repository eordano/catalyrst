import { useEffect, useRef, useState } from "react";

import { sendBridge, useBridgeState } from "../../overlay/bridge";
import { FLOATING_PANEL_TITLES } from "../../explorer/components/FloatingPanel";
import { portableSource } from "../../data/portableSource";
import "./smartwearablespanel.css";

type SmartWearablesPanelProps = {
  floating?: boolean;
};

function shortPid(pid: string): string {
  const bare = pid.replace(/^urn:decentraland:entity:/, "").split("?")[0] ?? pid;
  return bare.length > 14 ? `${bare.slice(0, 6)}\u{2026}${bare.slice(-6)}` : bare;
}

export default function SmartWearablesPanel({ floating = false }: SmartWearablesPanelProps = {}) {
  const portables = useBridgeState((s) => s.portables);
  const [source, setSource] = useState("");
  const [activating, setActivating] = useState(false);
  const [activationMessage, setActivationMessage] = useState("");
  const activate = async () => {
    if (activating) return;
    setActivationMessage("");
    try {
      const target = portableSource(source);
      if (!window.engine_console_command) throw new Error("Activation is unavailable in this client.");
      setActivating(true);
      await window.engine_console_command(`/spawn ${target}`);
      setActivationMessage("Experience requested. It will appear above when it starts.");
      setSource("");
    } catch (error) {
      setActivationMessage(error instanceof Error ? error.message : "The experience could not be activated.");
    } finally {
      setActivating(false);
    }
  };
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
    <div className={"swpanel" + (floating ? " swpanel--floating" : "")}>
      {floating ? null : <h2 className="swpanel__title">{FLOATING_PANEL_TITLES.portables}</h2>}
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
                  {p.name === "Basic Controller" && (
                    <span className="swpanel__hint">The explorer&#x2019;s system experience. It runs alongside the scene to provide shared controls.</span>
                  )}
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
      <form className="swpanel__activate" onSubmit={(e) => { e.preventDefault(); void activate(); }}>
        <label htmlFor="portable-source">Activate an experience</label>
        <p className="swpanel__hint">Enter its world name or realm URL. Only activate experiences you trust; they run alongside your current scene.</p>
        <input id="portable-source" value={source} onChange={(e) => setSource(e.target.value)} placeholder="experience.dcl.eth" disabled={activating} autoComplete="off" />
        <button className="swpanel__stop" type="submit" disabled={activating || !source.trim()}>{activating ? "Activating\u{2026}" : "Activate"}</button>
        {activationMessage && <p className="swpanel__hint" role="status">{activationMessage}</p>}
      </form>
    </div>
  );
}
