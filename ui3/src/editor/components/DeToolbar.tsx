import type { ComponentType } from "react";
import { useEffect, useRef, useState } from "react";
import type { EditorTool } from "../bus-protocol";
import type { EditorCamMode } from "../editor-bus";
import {
  IconBug,
  IconCamera,
  IconDots,
  IconMove,
  IconPause,
  IconPlay,
  IconRedo,
  IconRotate,
  IconScale,
  IconSelect,
  IconSidebarLeft,
  IconSidebarRight,
  IconStep,
  IconStop,
  IconUndo,
} from "./DeIcons";

const TOOLS: { id: EditorTool; Icon: ComponentType; title: string }[] = [
  { id: "select", Icon: IconSelect, title: "Select (Q)" },
  { id: "translate", Icon: IconMove, title: "Move (W)" },
  { id: "rotate", Icon: IconRotate, title: "Rotate (E)" },
  { id: "scale", Icon: IconScale, title: "Scale (R)" },
];

export interface DeToolbarProps {
  tool?: EditorTool;
  playing?: boolean;
  menuOpen?: boolean;
  camMode?: EditorCamMode;
  showAll?: boolean;
  saveLabel?: string;
  saveClass?: string;
  onTool?: (tool: EditorTool) => void;
  hideLeft?: boolean;
  onToggleLeft?: () => void;
  hideRight?: boolean;
  onToggleRight?: () => void;
  onPlay?: () => void;
  onPause?: () => void;
  onStep?: () => void;
  onStop?: () => void;
  onDebug?: () => void;
  debugActive?: boolean;
  onCamMode?: (mode: EditorCamMode) => void;
  onCameraSettings?: () => void;
  cameraPreset?: string;
  onUndo?: () => void;
  onRedo?: () => void;
  onCode?: () => void;
  codeActive?: boolean;
  live?: boolean;
  showGizmo?: boolean;
}

export function DeToolbar({
  tool = "translate",
  playing = false,
  menuOpen = false,
  camMode = "free",
  showAll = false,
  saveLabel = "Saved",
  saveClass = "ok",
  onTool,
  hideLeft = false,
  onToggleLeft,
  hideRight = false,
  onToggleRight,
  onPlay,
  onPause,
  onStep,
  onStop,
  onDebug = undefined,
  debugActive = false,
  onCamMode = undefined,
  onCameraSettings = undefined,
  cameraPreset = "blender",
  onUndo = undefined,
  onRedo = undefined,
  onCode = undefined,
  codeActive = false,
  live = false,
  showGizmo = !live,
}: DeToolbarProps) {
  const [open, setOpen] = useState(menuOpen);
  const [camOpen, setCamOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const camRef = useRef<HTMLDivElement | null>(null);
  const chip = playing
    ? { label: "Runtime", cls: "dim" }
    : { label: saveLabel, cls: saveClass };

  useEffect(() => {
    if (!open) return undefined;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    const onPointer = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onPointer);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onPointer);
    };
  }, [open]);

  useEffect(() => {
    if (!camOpen) return undefined;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setCamOpen(false);
    };
    const onPointer = (e: MouseEvent) => {
      if (camRef.current && !camRef.current.contains(e.target as Node)) setCamOpen(false);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onPointer);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onPointer);
    };
  }, [camOpen]);

  return (
    <div className="eui-panel eui-toolbar">
      <button
        className={"eui-btn icon" + (hideLeft ? " active" : "")}
        title={hideLeft ? "Show hierarchy" : "Hide hierarchy"}
        aria-label={hideLeft ? "Show hierarchy" : "Hide hierarchy"}
        onClick={onToggleLeft}
      >
        <IconSidebarLeft />
      </button>

      {showGizmo && (
        <div className="eui-tool-group">
          {TOOLS.map((t) => (
            <button
              key={t.id}
              title={t.title}
              aria-label={t.title}
              aria-pressed={tool === t.id}
              className={"eui-btn icon" + (tool === t.id ? " active" : "")}
              onClick={() => onTool?.(t.id)}
            >
              <t.Icon />
            </button>
          ))}
        </div>
      )}

      <div className="eui-tool-group">
        {(!live || onPlay || onPause) &&
          (playing ? (
            <button type="button" className="eui-btn icon active" title="Scene is running — pause" aria-label="Scene is running — pause" onClick={onPause}>
              <IconPause />
            </button>
          ) : (
            <>
              <button type="button" className="eui-btn icon" title="Run the scene" aria-label="Run the scene" onClick={onPlay}>
                <IconPlay />
              </button>
              <button type="button" className="eui-btn icon" title="Advance one tick" aria-label="Advance one tick" onClick={onStep}>
                <IconStep />
              </button>
            </>
          ))}
        {(!live || onStop) && (
          <button
            type="button"
            className="eui-btn icon"
            title={live ? "Stop the preview and return to editing" : "Restart the scene from tick 0"}
            aria-label={live ? "Stop the preview and return to editing" : "Restart the scene from tick 0"}
            onClick={onStop}
          >
            <IconStop />
          </button>
        )}
        {onDebug && (
          <button
            type="button"
            className={"eui-btn icon" + (debugActive ? " active" : "")}
            title="Debug — freeze the run and step tick by tick"
            aria-label="Debug — freeze the run and step tick by tick"
            aria-pressed={debugActive}
            onClick={onDebug}
          >
            <IconBug />
          </button>
        )}
      </div>

      <div className="eui-tool-group">
        <button className="eui-btn icon" title="Undo" aria-label="Undo" onClick={onUndo} disabled={!onUndo}>
          <IconUndo />
        </button>
        <button className="eui-btn icon" title="Redo" aria-label="Redo" onClick={onRedo} disabled={!onRedo}>
          <IconRedo />
        </button>
      </div>

      {onCode && (
        <div className="eui-tool-group">
          <button
            type="button"
            className={`eui-btn icon${codeActive ? " active" : ""}`}
            title="Edit code (file explorer + TypeScript editor)"
            aria-label="Edit code"
            aria-pressed={codeActive}
            onClick={onCode}
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M5.5 4 2 8l3.5 4M10.5 4 14 8l-3.5 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
        </div>
      )}

      {onCamMode && (
        <div ref={camRef} style={{ position: "relative", display: "flex" }}>
          <button
            className={"eui-btn icon" + (camOpen ? " active" : "")}
            title="Camera mode"
            aria-label="Camera mode"
            aria-haspopup="menu"
            aria-expanded={camOpen}
            onClick={() => setCamOpen((v) => !v)}
          >
            <IconCamera />
          </button>
          {camOpen && (
            <div className="eui-menu">
              <div className="eui-menu-label">Camera</div>
              <button className="eui-menu-item" onClick={() => { onCamMode("none"); setCamOpen(false); }}>
                Player camera<span className="hint">{camMode === "none" ? "●" : ""}</span>
              </button>
              <button className="eui-menu-item" onClick={() => { onCamMode("free"); setCamOpen(false); }}>
                Free fly<span className="hint">{camMode === "free" ? "●" : ""}</span>
              </button>
              <button className="eui-menu-item" onClick={() => { onCamMode("target"); setCamOpen(false); }}>
                Orbit selection<span className="hint">{camMode === "target" ? "●" : ""}</span>
              </button>
              {onCameraSettings && (
                <>
                  <div className="eui-menu-sep" />
                  <button className="eui-menu-item" onClick={() => { onCameraSettings(); setCamOpen(false); }}>
                    Camera settings…<span className="hint" style={{ textTransform: "capitalize" }}>{cameraPreset}</span>
                  </button>
                </>
              )}
            </div>
          )}
        </div>
      )}

      {!live && (
        <span className={"eui-autosave " + chip.cls} title="Auto-save status">
          <span className="dot" />
          {chip.label}
        </span>
      )}

      {!live && (
        <div ref={menuRef} style={{ position: "relative", display: "flex" }}>
          <button className={"eui-btn icon" + (open ? " active" : "")} title="More options" aria-label="More options" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)} disabled={live}>
            <IconDots />
          </button>
          {open && (
            <div className="eui-menu">
              <div className="eui-menu-label">Camera</div>
              <button className="eui-menu-item">Player camera<span className="hint">{camMode === "none" ? "●" : ""}</span></button>
              <button className="eui-menu-item">Free fly<span className="hint">{camMode === "free" ? "●" : ""}</span></button>
              <button className="eui-menu-item">Orbit selection<span className="hint">{camMode === "target" ? "●" : ""}</span></button>
              {camMode !== "none" && (
                <div style={{ display: "flex", gap: 2, padding: "2px 4px" }}>
                  {["+x", "-x", "+y", "-y", "+z", "-z"].map((a) => (
                    <button key={a} className="eui-btn" style={{ flex: 1, height: 24, padding: 0, fontSize: 11 }}>{a}</button>
                  ))}
                </div>
              )}
              <div className="eui-menu-sep" />
              <div className="eui-menu-label">Hierarchy</div>
              <button className="eui-menu-item">Show all entities<span className="hint">{showAll ? "on" : "off"}</span></button>
            </div>
          )}
        </div>
      )}

      <button
        className={"eui-btn icon" + (hideRight ? " active" : "")}
        title={hideRight ? "Show inspector" : "Hide inspector"}
        aria-label={hideRight ? "Show inspector" : "Hide inspector"}
        onClick={onToggleRight}
      >
        <IconSidebarRight />
      </button>
    </div>
  );
}
