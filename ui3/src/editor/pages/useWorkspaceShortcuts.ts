import { useEffect, useRef, useState } from "react";
import type { EditorTool } from "../bus-protocol";
import type { EditorCamMode } from "../editor-bus";
import { shortcutActionFor } from "../shortcuts";

interface WorkspaceShortcutHandlers {
  playing: boolean;
  camMode: EditorCamMode;
  debugOpen: boolean;
  live: boolean;
  onTool: (t: EditorTool) => void;
  onDelete?: (() => void) | undefined;
  onDuplicate?: (() => void) | undefined;
  onUndo?: (() => void) | undefined;
  onRedo?: (() => void) | undefined;
  onClearSelection?: (() => void) | undefined;
  onPlay?: (() => void) | undefined;
  onStepTick?: (() => void) | undefined;
}

export function useWorkspaceShortcuts(handlers: WorkspaceShortcutHandlers) {
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const shortcutRef = useRef<WorkspaceShortcutHandlers & { overlayOpen: boolean }>({
    playing: false,
    camMode: "target",
    overlayOpen: false,
    debugOpen: false,
    live: false,
    onTool: () => {},
  });
  shortcutRef.current = { ...handlers, overlayOpen: shortcutsOpen };

  useEffect(() => {
    if (typeof window === "undefined") return undefined;
    const onKey = (e: KeyboardEvent) => {
      const s = shortcutRef.current;
      const action = shortcutActionFor(e, {
        playing: s.playing,
        camMode: s.camMode,
        overlayOpen: s.overlayOpen,
        debugOpen: s.debugOpen,
        menuOrModalOpen: !!document.querySelector(
          ".modal__backdrop, .eui-menu, .eui-ctx, .eui-pop",
        ),
      });
      if (action === null) return;
      switch (action.type) {
        case "tool":
          e.preventDefault();
          s.onTool(action.tool);
          break;
        case "delete":
          if (s.onDelete) {
            e.preventDefault();
            s.onDelete();
          }
          break;
        case "duplicate":
          e.preventDefault();
          e.stopPropagation();
          s.onDuplicate?.();
          break;
        case "undo":
          e.preventDefault();
          e.stopPropagation();
          s.onUndo?.();
          break;
        case "redo":
          e.preventDefault();
          e.stopPropagation();
          s.onRedo?.();
          break;
        case "clear-selection":
          if (s.onClearSelection) {
            e.preventDefault();
            s.onClearSelection();
          }
          break;
        case "toggle-overlay":
          e.preventDefault();
          setShortcutsOpen((o) => !o);
          break;
        case "close-overlay":
          e.preventDefault();
          setShortcutsOpen(false);
          break;
        case "play":
          if (s.live) {
            e.preventDefault();
            if (!s.playing && s.onPlay) s.onPlay();
          }
          break;
        case "step-tick":
          if (s.onStepTick) {
            e.preventDefault();
            s.onStepTick();
          }
          break;
      }
    };
    window.addEventListener("keydown", onKey, { capture: true });
    return () => window.removeEventListener("keydown", onKey, { capture: true });
  }, []);

  return { shortcutsOpen, setShortcutsOpen };
}
