import { WORLD_CANVAS_ID } from "../keys";

export { WORLD_CANVAS_ID };

export type MovementCode = "KeyW" | "KeyA" | "KeyS" | "KeyD";
export type ModifierCode = "ShiftLeft" | "ControlLeft";
export type ActionCode = "Space" | "KeyE" | "KeyF";
export type CameraCode = "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight";
export type SyntheticKeyCode = MovementCode | ModifierCode | ActionCode | CameraCode;

export type LookStrategy = "pointer-motion" | "arrow-keys";

export type EngineInputLogEntry = {
  seq: number;
  at: number;
  type: string;
  detail: string;
};

export type EngineInputConfig = {
  canvasId: string;
  target: HTMLElement | null;
  lookStrategy: LookStrategy;
  lookSensitivity: number;
  lookInvertY: boolean;
  arrowHoldMs: number;
  onEvent: ((entry: EngineInputLogEntry) => void) | null;
};

export type EngineInputOptions = Partial<EngineInputConfig>;

export type EngineInput = {
  configure(patch: EngineInputOptions): void;
  focusCanvas(): boolean;
  setMovement(codes: readonly MovementCode[]): void;
  setModifier(code: ModifierCode, down: boolean): void;
  setAction(code: ActionCode, down: boolean): void;
  tapAction(code: ActionCode): void;
  beginLook(clientX: number, clientY: number): void;
  look(dx: number, dy: number): void;
  endLook(): void;
  primaryTap(clientX: number, clientY: number): void;
  releaseAll(): void;
  heldKeys(): SyntheticKeyCode[];
  activate(): void;
  dispose(): void;
};

type KeySpec = { key: string; keyCode: number; location: number };

const KEYS: Record<SyntheticKeyCode, KeySpec> = {
  KeyW: { key: "w", keyCode: 87, location: 0 },
  KeyA: { key: "a", keyCode: 65, location: 0 },
  KeyS: { key: "s", keyCode: 83, location: 0 },
  KeyD: { key: "d", keyCode: 68, location: 0 },
  ShiftLeft: { key: "Shift", keyCode: 16, location: 1 },
  ControlLeft: { key: "Control", keyCode: 17, location: 1 },
  Space: { key: " ", keyCode: 32, location: 0 },
  KeyE: { key: "e", keyCode: 69, location: 0 },
  KeyF: { key: "f", keyCode: 70, location: 0 },
  ArrowUp: { key: "ArrowUp", keyCode: 38, location: 0 },
  ArrowDown: { key: "ArrowDown", keyCode: 40, location: 0 },
  ArrowLeft: { key: "ArrowLeft", keyCode: 37, location: 0 },
  ArrowRight: { key: "ArrowRight", keyCode: 39, location: 0 },
};

const MOVEMENT_CODES: readonly MovementCode[] = ["KeyW", "KeyA", "KeyS", "KeyD"];
const ARROW_CODES: readonly CameraCode[] = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"];

const LOOK_POINTER_ID = 9001;
const TAP_POINTER_ID = 9002;

const NO_BUTTON = -1;
const LEFT_BUTTON = 0;
const RIGHT_BUTTON = 2;
const LEFT_BUTTONS = 1;
const RIGHT_BUTTONS = 2;
const NO_BUTTONS = 0;

const ARROW_EPSILON = 0.01;

const DEFAULTS: EngineInputConfig = {
  canvasId: WORLD_CANVAS_ID,
  target: null,
  lookStrategy: "pointer-motion",
  lookSensitivity: 1,
  lookInvertY: false,
  arrowHoldMs: 80,
  onEvent: null,
};

function pointerInit(patch: PointerEventInit): PointerEventInit {
  return {
    bubbles: true,
    cancelable: true,
    composed: true,
    pointerType: "mouse",
    isPrimary: true,
    ...patch,
  };
}

function motionEvent(
  x: number,
  y: number,
  dx: number,
  dy: number,
  buttons: number,
): PointerEvent {
  const init = pointerInit({
    pointerId: LOOK_POINTER_ID,
    button: NO_BUTTON,
    buttons,
    clientX: x,
    clientY: y,
    screenX: x,
    screenY: y,
    movementX: dx,
    movementY: dy,
  });
  return new PointerEvent("pointermove", {
    ...init,
    coalescedEvents: [new PointerEvent("pointermove", init)],
  });
}

function buttonEvent(
  type: "pointerdown" | "pointerup",
  pointerId: number,
  button: number,
  buttons: number,
  x: number,
  y: number,
): PointerEvent {
  return new PointerEvent(
    type,
    pointerInit({
      pointerId,
      button,
      buttons,
      clientX: x,
      clientY: y,
      screenX: x,
      screenY: y,
      movementX: 0,
      movementY: 0,
    }),
  );
}

function keyEvent(type: "keydown" | "keyup", code: SyntheticKeyCode): KeyboardEvent {
  const spec = KEYS[code];
  return new KeyboardEvent(type, {
    key: spec.key,
    code,
    keyCode: spec.keyCode,
    which: spec.keyCode,
    location: spec.location,
    bubbles: true,
    cancelable: true,
    composed: true,
  });
}

export function createEngineInput(options: EngineInputOptions = {}): EngineInput {
  const config: EngineInputConfig = { ...DEFAULTS, ...options };
  const held = new Set<SyntheticKeyCode>();
  const arrows = new Set<CameraCode>();
  let lookHeld = false;
  let lastX = 0;
  let lastY = 0;
  let seq = 0;
  let arrowTimer: ReturnType<typeof setTimeout> | null = null;
  let disposed = false;

  function log(type: string, detail: string) {
    const sink = config.onEvent;
    if (!sink) return;
    seq += 1;
    sink({ seq, at: Date.now(), type, detail });
  }

  function target(): HTMLElement | null {
    if (config.target) return config.target;
    if (typeof document === "undefined") return null;
    return document.getElementById(config.canvasId);
  }

  function send(event: Event, type: string, detail: string): boolean {
    const el = target();
    if (!el) {
      log("dropped", `${type} (${detail}) \u{2014} no target`);
      return false;
    }
    el.dispatchEvent(event);
    log(type, detail);
    return true;
  }

  function pressKey(code: SyntheticKeyCode) {
    if (disposed || held.has(code)) return;
    if (!send(keyEvent("keydown", code), "keydown", code)) return;
    held.add(code);
  }

  function releaseKey(code: SyntheticKeyCode) {
    if (!held.delete(code)) return;
    send(keyEvent("keyup", code), "keyup", code);
  }

  function clearArrowTimer() {
    if (arrowTimer === null) return;
    clearTimeout(arrowTimer);
    arrowTimer = null;
  }

  function releaseArrows() {
    clearArrowTimer();
    for (const code of arrows) releaseKey(code);
    arrows.clear();
  }

  function lookByArrows(dx: number, dy: number) {
    const next = new Set<CameraCode>();
    if (dx > ARROW_EPSILON) next.add("ArrowRight");
    if (dx < -ARROW_EPSILON) next.add("ArrowLeft");
    if (dy > ARROW_EPSILON) next.add("ArrowDown");
    if (dy < -ARROW_EPSILON) next.add("ArrowUp");
    for (const code of ARROW_CODES) {
      if (next.has(code)) {
        arrows.add(code);
        pressKey(code);
      } else if (arrows.delete(code)) {
        releaseKey(code);
      }
    }
    clearArrowTimer();
    if (arrows.size > 0) arrowTimer = setTimeout(releaseArrows, config.arrowHoldMs);
  }

  function pressCameraLock(x: number, y: number) {
    if (lookHeld) return;
    lookHeld = true;
    send(
      buttonEvent("pointerdown", LOOK_POINTER_ID, RIGHT_BUTTON, RIGHT_BUTTONS, x, y),
      "pointerdown",
      "button=2 cameraLock",
    );
  }

  function releaseEverything() {
    clearArrowTimer();
    arrows.clear();
    for (const code of [...held]) releaseKey(code);
    releaseCameraLock();
  }

  function releaseCameraLock() {
    if (!lookHeld) return;
    lookHeld = false;
    send(
      buttonEvent("pointerup", LOOK_POINTER_ID, RIGHT_BUTTON, NO_BUTTONS, lastX, lastY),
      "pointerup",
      "button=2 cameraLock",
    );
  }

  return {
    configure(patch: EngineInputOptions) {
      Object.assign(config, patch);
    },

    focusCanvas(): boolean {
      const el = target();
      if (!el) return false;
      if (document.activeElement !== el) {
        el.style.outline = "none";
        el.focus({ preventScroll: true });
      }
      return document.activeElement === el;
    },

    setMovement(codes: readonly MovementCode[]) {
      for (const code of MOVEMENT_CODES) {
        if (codes.includes(code)) pressKey(code);
        else releaseKey(code);
      }
    },

    setModifier(code: ModifierCode, down: boolean) {
      if (down) pressKey(code);
      else releaseKey(code);
    },

    setAction(code: ActionCode, down: boolean) {
      if (down) pressKey(code);
      else releaseKey(code);
    },

    tapAction(code: ActionCode) {
      pressKey(code);
      releaseKey(code);
    },

    beginLook(clientX: number, clientY: number) {
      lastX = clientX;
      lastY = clientY;
      if (config.lookStrategy === "pointer-motion") pressCameraLock(clientX, clientY);
    },

    look(dx: number, dy: number) {
      if (disposed) return;
      const sx = dx * config.lookSensitivity;
      const sy = dy * config.lookSensitivity * (config.lookInvertY ? -1 : 1);
      if (sx === 0 && sy === 0) return;
      lastX += dx;
      lastY += dy;
      if (config.lookStrategy === "arrow-keys") {
        lookByArrows(sx, sy);
        return;
      }
      pressCameraLock(lastX, lastY);
      send(
        motionEvent(lastX, lastY, sx, sy, RIGHT_BUTTONS),
        "pointermove",
        `movement=${sx.toFixed(1)},${sy.toFixed(1)}`,
      );
    },

    endLook() {
      releaseArrows();
      releaseCameraLock();
    },

    primaryTap(clientX: number, clientY: number) {
      lastX = clientX;
      lastY = clientY;
      send(
        motionEvent(clientX, clientY, 0, 0, NO_BUTTONS),
        "pointermove",
        `cursor=${Math.round(clientX)},${Math.round(clientY)}`,
      );
      send(
        buttonEvent("pointerdown", TAP_POINTER_ID, LEFT_BUTTON, LEFT_BUTTONS, clientX, clientY),
        "pointerdown",
        "button=0 IaPointer",
      );
      send(
        buttonEvent("pointerup", TAP_POINTER_ID, LEFT_BUTTON, NO_BUTTONS, clientX, clientY),
        "pointerup",
        "button=0 IaPointer",
      );
    },

    releaseAll: releaseEverything,

    heldKeys(): SyntheticKeyCode[] {
      return [...held];
    },

    activate() {
      disposed = false;
    },

    dispose() {
      releaseEverything();
      disposed = true;
      config.onEvent = null;
    },
  };
}
