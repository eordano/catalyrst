export const WORLD_CANVAS_ID = "mygame-canvas";

export type WorldKeyCode =
  | "KeyW"
  | "KeyA"
  | "KeyS"
  | "KeyD"
  | "ShiftLeft"
  | "ControlLeft"
  | "Space"
  | "KeyE"
  | "KeyF"
  | "ArrowUp"
  | "ArrowDown"
  | "ArrowLeft"
  | "ArrowRight";

export const WORLD_KEY_CODES: readonly WorldKeyCode[] = [
  "KeyW",
  "KeyA",
  "KeyS",
  "KeyD",
  "ShiftLeft",
  "ControlLeft",
  "Space",
  "KeyE",
  "KeyF",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
];

const WORLD_KEY_CODE_SET: ReadonlySet<string> = new Set<string>(WORLD_KEY_CODES);

export function isWorldKeyCode(code: string | null | undefined): code is WorldKeyCode {
  return code != null && WORLD_KEY_CODE_SET.has(code);
}

export function isSynthesizedWorldKey(e: KeyboardEvent): boolean {
  return e.isTrusted === false && isWorldKeyCode(e.code);
}

export function getWorldCanvas(): HTMLElement | null {
  if (typeof document === "undefined") return null;
  return document.getElementById(WORLD_CANVAS_ID);
}
