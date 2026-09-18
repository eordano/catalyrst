import type { EditorTool } from "./bus-protocol";

type EditorShortcutAction =
  | { type: "tool"; tool: EditorTool }
  | { type: "delete" }
  | { type: "duplicate" }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "clear-selection" }
  | { type: "toggle-overlay" }
  | { type: "close-overlay" }
  | { type: "play" }
  | { type: "step-tick" };

export interface EditorShortcutContext {
  playing: boolean;
  camMode: string;
  overlayOpen: boolean;
  menuOrModalOpen: boolean;
  debugOpen?: boolean;
}

export function isTypingTarget(
  e: Pick<KeyboardEvent, "composedPath" | "target">,
): boolean {
  let el: unknown = e.target;
  try {
    const path = typeof e.composedPath === "function" ? e.composedPath() : [];
    if (path.length > 0) el = path[0];
  } catch {
  }
  const node = el as { tagName?: string; isContentEditable?: boolean; closest?: (selector: string) => Element | null } | null;
  const tag = node?.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    node?.isContentEditable === true ||
    !!node?.closest?.('[role="textbox"], .monaco-editor')
  );
}

const TOOL_KEYS: Record<string, EditorTool> = {
  q: "select",
  w: "translate",
  e: "rotate",
  r: "scale",
};

export function shortcutActionFor(
  e: Pick<
    KeyboardEvent,
    "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "composedPath" | "target"
  >,
  ctx: EditorShortcutContext,
): EditorShortcutAction | null {
  if (isTypingTarget(e)) return null;
  const key = e.key;
  if (typeof key !== "string") return null;
  const lower = key.toLowerCase();
  const mod = e.metaKey || e.ctrlKey;

  if (ctx.overlayOpen) {
    if (key === "?" || key === "Escape") return { type: "close-overlay" };
    return null;
  }
  if (ctx.menuOrModalOpen) return null;

  if (mod && !e.altKey && lower === "z") {
    return e.shiftKey ? { type: "redo" } : { type: "undo" };
  }
  if (mod && !e.altKey && lower === "d") return { type: "duplicate" };

  if (key === "F5") return { type: "play" };
  if (key === "?" && !mod) return { type: "toggle-overlay" };
  if (key === "Escape") return { type: "clear-selection" };
  if ((key === "Delete" || key === "Backspace") && !mod) return { type: "delete" };

  if (key === "." && !mod && !e.altKey && ctx.debugOpen === true) return { type: "step-tick" };

  if (!mod && !e.altKey && /^[a-z]$/.test(lower)) {
    if (ctx.playing || ctx.camMode === "free") return null;
    const tool = TOOL_KEYS[lower];
    if (tool !== undefined) return { type: "tool", tool };
  }
  return null;
}

export const FORWARDED_KEYS = new Set([
  "q",
  "w",
  "e",
  "r",
  "z",
  "d",
  "F5",
  "?",
  ".",
  "Delete",
  "Backspace",
  "Escape",
]);

const isForwardedKey = (key: string): boolean =>
  FORWARDED_KEYS.has(key) || FORWARDED_KEYS.has(key.toLowerCase());

export function forwardEngineKeys(
  engineWindow: Window | null | undefined,
  options: { iframe?: HTMLIFrameElement | null; isEditingEnabled?: () => boolean } = {},
): () => void {
  if (!engineWindow || typeof engineWindow.addEventListener !== "function") return () => {};
  const onKey = (e: KeyboardEvent) => {
    if (isTypingTarget(e) || typeof e.key !== "string") return;
    const mod = e.metaKey || e.ctrlKey;
    if (!mod && !isForwardedKey(e.key)) return;
    if (!mod && e.key !== "F5" && options.isEditingEnabled?.() === false) return;
    if (mod || e.key === "F5") e.preventDefault();
    const forwarded = new KeyboardEvent(e.type, {
      key: e.key, code: e.code, keyCode: e.keyCode, which: e.which,
      shiftKey: e.shiftKey, ctrlKey: e.ctrlKey, metaKey: e.metaKey, altKey: e.altKey,
      bubbles: true, cancelable: true,
    } as KeyboardEventInit);
    (window.document.body ?? window.document).dispatchEvent(forwarded);
  };
  const onPointerDown = () => options.iframe?.focus();
  for (const type of ["keydown", "keyup"]) engineWindow.addEventListener(type, onKey as EventListener, true);
  engineWindow.addEventListener("pointerdown", onPointerDown, true);
  return () => {
    for (const type of ["keydown", "keyup"]) engineWindow.removeEventListener(type, onKey as EventListener, true);
    engineWindow.removeEventListener("pointerdown", onPointerDown, true);
  };
}

interface ShortcutItem {
  combo: string;
  label: string;
}

interface ShortcutGroup {
  title: string;
  items: ShortcutItem[];
}

export function shortcutGroups(opts?: { preset?: string; mac?: boolean }): ShortcutGroup[] {
  const mac =
    opts?.mac ??
    (typeof navigator !== "undefined" && /mac/i.test(navigator.platform || ""));
  const mod = mac ? "\u{2318}" : "Ctrl";
  const preset =
    opts?.preset === "maya" || opts?.preset === "blender-lmb" ? opts.preset : "blender";
  const mouse: ShortcutItem[] =
    preset === "maya"
      ? [
          { combo: "Alt LMB", label: "Orbit" },
          { combo: "Alt MMB", label: "Pan" },
          { combo: "Alt RMB", label: "Dolly" },
        ]
      : [
          { combo: "MMB drag", label: "Orbit" },
          { combo: "\u{21E7} MMB", label: "Pan" },
          { combo: "Ctrl MMB", label: "Dolly" },
          ...(preset === "blender-lmb"
            ? [{ combo: "Alt LMB", label: "Orbit (LMB variant)" }]
            : []),
        ];
  return [
    {
      title: "Tools \u{2014} edit camera, not while playing",
      items: [
        { combo: "Q", label: "Select" },
        { combo: "W", label: "Move (translate)" },
        { combo: "E", label: "Rotate" },
        { combo: "R", label: "Scale" },
      ],
    },
    {
      title: "Edit",
      items: [
        { combo: `${mod} Z`, label: "Undo" },
        { combo: `${mod} \u{21E7} Z`, label: "Redo" },
        { combo: `${mod} D`, label: "Duplicate selection" },
        { combo: "Del / \u{232B}", label: "Delete selection" },
      ],
    },
    {
      title: "Camera \u{2014} mouse",
      items: [...mouse, { combo: "Scroll", label: "Zoom (dolly)" }],
    },
    {
      title: "Camera \u{2014} keys (viewport focused)",
      items: [
        { combo: "F", label: "Focus selection" },
        { combo: "`", label: "Toggle fly camera" },
        { combo: "W A S D", label: "Fly move (fly camera on / playing)" },
        { combo: "Numpad 1 / 3 / 7", label: "Front / right / top view" },
        { combo: "Ctrl Numpad 1 / 3 / 7", label: "Opposite view" },
        { combo: "Numpad 5", label: "Ortho \u{21C4} perspective" },
      ],
    },
    {
      title: "Playback",
      items: [
        { combo: "F5", label: "Play (preview \u{2014} Stop restores the scene)" },
        { combo: ".", label: "Step one tick (debug panel open \u{2014} toolbar Debug while playing)" },
      ],
    },
    {
      title: "General",
      items: [
        { combo: "Esc", label: "Clear selection / close" },
        { combo: "?", label: "Show / hide this list" },
      ],
    },
  ];
}
