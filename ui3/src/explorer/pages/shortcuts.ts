import { EXPLORE_TABS } from "../frames/ExploreChrome";

export type Shortcut = { action: string; keys: string[] };

type ShortcutGroup = { id: string; title: string; rows: Shortcut[] };

const MOVEMENT_SHORTCUTS: Shortcut[] = [
  { action: "Move", keys: ["W", "A", "S", "D"] },
  { action: "Jump", keys: ["Space"] },
  { action: "Run (hold)", keys: ["Shift"] },
  { action: "Walk (hold)", keys: ["Ctrl"] },
  { action: "Interact", keys: ["E"] },
  { action: "Secondary action", keys: ["F"] },
  { action: "Scene actions", keys: ["1", "2", "3", "4"] },
  { action: "Point at", keys: ["Q", "/", "Middle Mouse"] },
];

const CAMERA_SHORTCUTS: Shortcut[] = [
  { action: "Look around", keys: ["Mouse"] },
  { action: "Mouse look (hold)", keys: ["Right Mouse"] },
  { action: "Pan camera (drag)", keys: ["Left Mouse"] },
  { action: "Switch camera", keys: ["V"] },
  { action: "Switch shoulder", keys: ["."] },
  { action: "Free camera", keys: ["F11"] },
  { action: "Turn the camera", keys: ["\u2190", "\u2191", "\u2193", "\u2192"] },
  { action: "Zoom", keys: ["Scroll"] },
  { action: "Roll camera", keys: ["T", "/", "G"] },
  { action: "Hide the interface", keys: ["U"] },
  { action: "Hide player names", keys: ["N"] },
];

const SOCIAL_SHORTCUTS: Shortcut[] = [
  { action: "Open chat", keys: ["Enter"] },
  { action: "Microphone", keys: ["P"] },
  { action: "Emote wheel", keys: ["B"] },
  { action: "Map", keys: ["M", "/", "Tab"] },
  { action: "Close a panel", keys: ["Esc"] },
];

const PANEL_SHORTCUTS: Shortcut[] = EXPLORE_TABS.filter(t => t.hint).map((t) => ({
  action: t.label,
  keys: [t.hint],
}));

export const CAMERA_MODE_SHORTCUTS: Shortcut[] = [
  { action: "Take a photo", keys: ["Space"] },
  { action: "Move camera", keys: ["W", "A", "S", "D"] },
  { action: "Up / Down", keys: ["Q", "/", "E"] },
  { action: "Rotate", keys: ["Right Mouse"] },
  { action: "Zoom", keys: ["Scroll"] },
  { action: "Adjust speed", keys: ["Shift"] },
  { action: "Roll camera", keys: [",", "/", "."] },
  { action: "Reset roll", keys: ["R"] },
  { action: "Toggle UI", keys: ["H"] },
  { action: "Exit camera", keys: ["Esc"] },
];

export const HELP_SHORTCUT_GROUPS: ShortcutGroup[] = [
  { id: "move", title: "Getting around", rows: MOVEMENT_SHORTCUTS },
  { id: "camera", title: "Camera", rows: CAMERA_SHORTCUTS },
  { id: "social", title: "Chat & social", rows: SOCIAL_SHORTCUTS },
  { id: "panels", title: "Open a panel", rows: PANEL_SHORTCUTS },
  { id: "photo", title: "Photo mode", rows: CAMERA_MODE_SHORTCUTS },
];
