import { useEffect, useRef, useState } from "react";
import Modal from "@ui/components/Modal";
import DeDeviceTelemetry from "@ui/editor/components/DeDeviceTelemetry";
import { applyDeviceEntries, emptyDeviceTelemetry, type DeviceTelemetry } from "@ui/editor/device-debug";
import type { SdkDebugDescriptor, SdkDebugSession, SdkProjectConnection } from "@data/lib/fs/sdk-project";
import "./device-preview.css";

export default function DevicePreviewPanel({ project, onClose }: { project: SdkProjectConnection; onClose: () => void }) {
  return <DevicePreviewConnection key={project.url} project={project} onClose={onClose} />;
}

function DevicePreviewConnection({ project, onClose }: { project: SdkProjectConnection; onClose: () => void }) {
  const [descriptor, setDescriptor] = useState<SdkDebugDescriptor | null>(null);
  const [sessions, setSessions] = useState<SdkDebugSession[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [entries, setEntries] = useState<{ sessionId: number; text: string }[]>([]);
  const [telemetry, setTelemetry] = useState<Record<number, DeviceTelemetry>>({});
  const [error, setError] = useState<string | null>(null);
  const [commandPending, setCommandPending] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const connectionRef = useRef<AbortController | null>(null);
  const selectedRef = useRef(selected);
  selectedRef.current = selected;
  useEffect(() => {
    const controller = new AbortController();
    connectionRef.current = controller;
    setError(null); setEntries([]); setTelemetry({}); setDescriptor(null); setSessions([]); setSelected(null); setNotice(null); setCommandPending(false);
    void project.debug.descriptor().then((value) => { if (!controller.signal.aborted) setDescriptor(value); })
      .catch((error: unknown) => { if (!controller.signal.aborted) setError(String(error)); });
    void project.debug.events((event) => {
      if (controller.signal.aborted) return;
      if (event.type === "sessions") {
        const ids = new Set(event.sessions.map(session => session.id));
        setTelemetry(previous => Object.fromEntries(Object.entries(previous).filter(([id]) => ids.has(Number(id)))));
        setEntries(previous => previous.filter(entry => ids.has(entry.sessionId)));
        setSessions(event.sessions);
        setSelected((previous) => event.sessions.some((session) => session.id === previous)
          ? previous : event.sessions.find((session) => session.status === "active")?.id ?? event.sessions[0]?.id ?? null);
      } else if (event.type === "entries") {
        setTelemetry(previous => ({ ...previous, [event.sessionId]: applyDeviceEntries(previous[event.sessionId] ?? emptyDeviceTelemetry(), event.entries) }));
        setEntries((previous) => [...previous, ...event.entries.map((entry) => ({ sessionId: event.sessionId, text: JSON.stringify(entry, null, 2) }))].slice(-500));
      } else {
        setTelemetry({});
        setNotice(`${event.missed} log batches were missed. Entity inspection restarted from the latest device events.`);
      }
    }, controller.signal).catch((error: unknown) => { if (!controller.signal.aborted) setError(error instanceof Error ? error.message : String(error)); });
    return () => controller.abort();
  }, [project, attempt]);
  const active = sessions.find((session) => session.id === selected);
  const command = async (cmd: "pause" | "resume" | "reload_scene") => {
    const connection = connectionRef.current;
    if (!active || active.status !== "active" || commandPending || !connection || connection.signal.aborted) return;
    setCommandPending(true); setError(null); setNotice(null);
    try {
      await project.debug.command(active.id, cmd);
      if (connection.signal.aborted || selectedRef.current !== active.id) return;
      setNotice(`${cmd === "reload_scene" ? "Reload" : cmd === "pause" ? "Pause" : "Resume"} acknowledged by ${active.deviceName || "the device"}.`);
    } catch (error) { if (!connection.signal.aborted && selectedRef.current === active.id) setError(error instanceof Error ? error.message : String(error)); }
    finally { if (!connection.signal.aborted) setCommandPending(false); }
  };
  return <Modal onClose={onClose} showClose={false} width={900} ariaLabel="Device preview">
    <section className="device-preview">
      <header><h2>Device preview</h2><button type="button" className="editor-wizard__btn" onClick={onClose}>Close</button></header>
      <p>Preview this SDK project in a desktop or mobile client. Connected devices send their logs here.</p>
      {descriptor ? <div className="device-preview__launch">
        <a className="editor-wizard__btn" href={descriptor.nativeUrl}>Open desktop preview</a>
        <a className="editor-wizard__btn" href={descriptor.multiInstanceUrl}>Open another instance</a>
        {descriptor.mobileQr && <figure>
          <img width={128} height={128} src={descriptor.mobileQr} alt="Scan to open this project in the mobile app" />
          <figcaption>Scan with the mobile app on the same network.</figcaption>
        </figure>}
        {!descriptor.mobileUrl && <p>No local network address is available for mobile preview.</p>}
      </div> : !error && <p role="status">Loading preview links&#x2026;</p>}
      <div className="device-preview__controls">
        <label>Device<select value={selected ?? ""} disabled={commandPending} onChange={(event) => { setSelected(Number(event.target.value)); setNotice(null); }}>
          {!sessions.length && <option value="">No device connected</option>}
          {sessions.map((session) => <option key={session.id} value={session.id}>{session.deviceName || `Device ${session.id}`} &#xb7; {session.status === "active" ? "Connected" : "Disconnected"}</option>)}
        </select></label>
        {(["pause", "resume", "reload_scene"] as const).map((cmd) => <button key={cmd} type="button" className="editor-wizard__btn" disabled={active?.status !== "active" || commandPending} onClick={() => void command(cmd)}>{cmd === "reload_scene" ? "Reload scene" : cmd === "pause" ? "Pause" : "Resume"}</button>)}
      </div>
      {notice && <p role="status">{notice}</p>}
      {error && <div role="alert"><p>{error}</p><button type="button" className="editor-wizard__btn" onClick={() => setAttempt((value) => value + 1)}>Reconnect logs</button></div>}
      <DeDeviceTelemetry key={selected ?? "none"} data={selected === null ? emptyDeviceTelemetry() : telemetry[selected] ?? emptyDeviceTelemetry()} />
      <div className="device-preview__logs" role="log" aria-label="Device logs">
        {entries.filter((entry) => entry.sessionId === selected).map((entry, index) => <pre key={index}>{entry.text}</pre>)}
        {!entries.some((entry) => entry.sessionId === selected) && <p>{sessions.length ? "Waiting for logs from this device\u2026" : "Open a preview to connect a device."}</p>}
      </div>
    </section>
  </Modal>;
}
