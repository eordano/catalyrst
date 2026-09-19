import { useEffect, useRef, useState } from "react";
import type { DeWorkspaceCode } from "../types";
import { attachDesignerBridge, type DesignerRuntime } from "../ui-designer-bridge";

type Project = NonNullable<DeWorkspaceCode["project"]>;
export default function DeUIDesigner({ project, onClose }: { project: Project; onClose(): void }) {
  const frame = useRef<HTMLIFrameElement>(null);
  const timer = useRef<number | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let disposed = false;
    let detach: (() => void) | undefined;
    timer.current = window.setTimeout(() => { if (!disposed) setError("The UI Designer did not finish loading. Check the SDK terminal and retry."); }, 30000);
    setError(null); setLoading(true);
    void (async () => {
      if (!project.uiDesigner) throw new Error("This SDK project does not provide the upstream UI Designer.");
      const url = new URL(project.uiDesigner.url);
      const runtimeUrl = new URL(project.uiDesigner.runtimeUrl);
      if (url.origin !== runtimeUrl.origin) throw new Error("The UI Designer runtime must come from the project server.");
      const runtime = await import(/* @vite-ignore */ runtimeUrl.href) as DesignerRuntime;
      if (disposed || !frame.current) return;
      if (typeof runtime.parse !== "function" || !runtime.RPC || !runtime.Transport) throw new Error("The SDK UI Designer runtime is incomplete. Update the SDK and retry.");
      url.searchParams.set("parent", window.location.origin);
      url.searchParams.set("uiDesignerOpen", "true");
      url.searchParams.set("uiEditorEnabled", "true");
      url.searchParams.set("uiEditorSupported", "true");
      frame.current.src = url.href;
      detach = attachDesignerBridge(frame.current, project, runtime, setError);
    })().catch(error => { if (!disposed) { setError(error instanceof Error ? error.message : String(error)); setLoading(false); } });
    return () => { disposed = true; window.clearTimeout(timer.current); detach?.(); };
  }, [project, attempt]);
  return <section className="eui-panel" aria-label="UI Designer" style={{ position: "absolute", inset: "96px 12px 12px", zIndex: 40, display: "flex", flexDirection: "column", pointerEvents: "auto", background: "var(--eui-panel, #202024)" }}>
    <header style={{ display: "flex", alignItems: "center", gap: 12, padding: 10 }}>
      <strong>UI Designer</strong><span style={{ flex: 1 }}>Visual edits save to your scene&#x2019;s TSX files.</span>
      <button className="eui-btn" onClick={onClose}>Close UI Designer</button>
    </header>
    {error && <div role="alert" style={{ padding: 10 }}>{error} <button className="eui-btn" onClick={() => setAttempt(value => value + 1)}>Retry</button></div>}
    {loading && <p role="status" style={{ padding: 10 }}>Loading upstream UI Designer&#x2026;</p>}
    <iframe ref={frame} title="Upstream UI Designer" onLoad={() => { if (frame.current?.src) { setLoading(false); window.clearTimeout(timer.current); } }} style={{ width: "100%", flex: 1, border: 0, minHeight: 0 }} />
  </section>;
}
