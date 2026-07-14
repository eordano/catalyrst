
import { normalizeCameraPrefs } from "./camera-prefs";

const ORBIT_GAIN = 0.005;
const PAN_GAIN = 0.01;
const DOLLY_DRAG_GAIN = 0.005;
const WHEEL_STEP = 0.12;

function classifyGesture(preset, button, mods) {
  const { alt, shift, ctrl } = mods;
  if (preset === "blender" || preset === "blender-lmb") {
    if (button === 1) {
      if (shift) return "pan";
      if (ctrl) return "dolly";
      return "orbit";
    }
    if (preset === "blender-lmb" && button === 0 && alt) {
      if (shift) return "pan";
      if (ctrl) return "dolly";
      return "orbit";
    }
    return null;
  }
  if (preset === "maya") {
    if (!alt) return null;
    if (button === 0) return "orbit";
    if (button === 1) return "pan";
    if (button === 2) return "dolly";
  }
  return null;
}

function axisForNumpad(code, ctrl) {
  switch (code) {
    case "Numpad1":
      return ctrl ? "+z" : "-z";
    case "Numpad3":
      return ctrl ? "-x" : "+x";
    case "Numpad7":
      return ctrl ? "-y" : "+y";
    default:
      return null;
  }
}

export function attachCameraInput(contentWindow, bus, getPrefs, getCtx) {
  if (!contentWindow || !bus || typeof bus.setCameraInput !== "function") {
    return () => {};
  }

  let doc;
  try {
    doc = contentWindow.document;
    void contentWindow.location.href;
    if (!doc) throw new Error("no document");
  } catch {
    console.warn(
      "[camera-input] /_play iframe unreachable (cross-origin?); Blender/Maya gestures disabled \u{2014} scene-side RMB-look + WASD still work",
    );
    return () => {};
  }

  const prefs = () => normalizeCameraPrefs(typeof getPrefs === "function" ? getPrefs() : null);
  const ctx = () =>
    typeof getCtx === "function" ? getCtx() || {} : {};

  let gesture = null;
  let lastX = 0;
  let lastY = 0;

  let pendingDelta = null;
  let flushScheduled = false;
  let detached = false;
  const flushCameraInput = () => {
    flushScheduled = false;
    const d = pendingDelta;
    pendingDelta = null;
    if (!detached && d) bus.setCameraInput(d);
  };
  const queueCameraInput = (d) => {
    if (!pendingDelta) pendingDelta = {};
    for (const k of Object.keys(d)) {
      pendingDelta[k] = (pendingDelta[k] || 0) + d[k];
    }
    if (flushScheduled) return;
    flushScheduled = true;
    if (typeof window !== "undefined" && typeof window.requestAnimationFrame === "function") {
      window.requestAnimationFrame(flushCameraInput);
    } else {
      setTimeout(flushCameraInput, 16);
    }
  };

  const resetGesture = () => {
    gesture = null;
  };

  const onPointerDown = (e) => {
    const g = classifyGesture(prefs().preset, e.button, {
      alt: e.altKey,
      shift: e.shiftKey,
      ctrl: e.ctrlKey || e.metaKey,
    });
    if (!g) return;
    e.preventDefault();
    e.stopPropagation();
    gesture = g;
    lastX = e.clientX;
    lastY = e.clientY;
    if (ctx().camMode === "none") bus.setCamMode("target");
  };

  const onPointerMove = (e) => {
    if (!gesture) return;
    e.preventDefault();
    e.stopPropagation();
    const dx = e.clientX - lastX;
    const dy = e.clientY - lastY;
    lastX = e.clientX;
    lastY = e.clientY;
    if (dx === 0 && dy === 0) return;
    const p = prefs();
    if (gesture === "orbit") {
      queueCameraInput({
        orbitYaw: dx * ORBIT_GAIN * p.sensitivity.orbit,
        orbitPitch: dy * ORBIT_GAIN * p.sensitivity.orbit * (p.invertY ? -1 : 1),
      });
    } else if (gesture === "pan") {
      queueCameraInput({
        panX: dx * PAN_GAIN * p.sensitivity.pan,
        panY: dy * PAN_GAIN * p.sensitivity.pan,
      });
    } else if (gesture === "dolly") {
      queueCameraInput({ dolly: dx * DOLLY_DRAG_GAIN * p.sensitivity.zoom });
    }
  };

  const onPointerUp = (e) => {
    if (!gesture) return;
    e.preventDefault();
    e.stopPropagation();
    resetGesture();
  };

  const onWheel = (e) => {
    e.preventDefault();
    e.stopPropagation();
    const p = prefs();
    const dir = e.deltaY < 0 ? 1 : -1;
    if (ctx().camMode === "none") bus.setCamMode("target");
    queueCameraInput({ dolly: dir * WHEEL_STEP * p.sensitivity.zoom });
  };

  const onDblClick = (e) => {
    const { activeId } = ctx();
    if (activeId == null) return;
    e.preventDefault();
    e.stopPropagation();
    bus.focus(activeId, true, true);
  };

  // The engine canvas preventDefault()s pointerdown, which suppresses ALL
  // compatibility mouse events (mousedown/mousemove/mouseup/dblclick) per the
  // pointer-events spec -- so double-click must be detected from the pointer
  // stream itself. Two primary-button pointerups, close in time and space.
  let lastPrimaryUp = null;
  const onPrimaryUpForDbl = (e) => {
    if (e.button !== 0 || gesture) return;
    const now = Date.now();
    const prev = lastPrimaryUp;
    lastPrimaryUp = { t: now, x: e.clientX, y: e.clientY };
    if (
      prev &&
      now - prev.t < 400 &&
      Math.abs(e.clientX - prev.x) < 6 &&
      Math.abs(e.clientY - prev.y) < 6
    ) {
      lastPrimaryUp = null;
      onDblClick(e);
    }
  };

  const onKeyDown = (e) => {
    const code = e.code;
    const ctrl = e.ctrlKey || e.metaKey;
    if (code === "KeyF") {
      const { activeId } = ctx();
      if (activeId != null) {
        bus.focus(activeId, true);
        e.preventDefault();
        e.stopPropagation();
      }
      return;
    }
    const axis = axisForNumpad(code, ctrl);
    if (axis) {
      const cm = ctx().camMode;
      bus.orientAxis(cm && cm !== "none" ? cm : "target", axis);
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (code === "Numpad5") {
      bus.toggleOrtho();
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (code === "Backquote") {
      const cm = ctx().camMode;
      bus.setCamMode(cm === "free" ? "target" : "free");
      e.preventDefault();
      e.stopPropagation();
      return;
    }
  };

  const opts = true;
  contentWindow.addEventListener("pointerdown", onPointerDown, opts);
  contentWindow.addEventListener("pointermove", onPointerMove, opts);
  contentWindow.addEventListener("pointerup", onPointerUp, opts);
  contentWindow.addEventListener("pointerup", onPrimaryUpForDbl, opts);
  contentWindow.addEventListener("wheel", onWheel, { capture: true, passive: false });
  contentWindow.addEventListener("dblclick", onDblClick, opts);
  contentWindow.addEventListener("keydown", onKeyDown, opts);
  contentWindow.addEventListener("blur", resetGesture, opts);
  const hostUp = () => resetGesture();
  if (typeof window !== "undefined") {
    window.addEventListener("pointerup", hostUp, true);
    window.addEventListener("blur", hostUp, true);
  }

  return () => {
    try {
      contentWindow.removeEventListener("pointerdown", onPointerDown, opts);
      contentWindow.removeEventListener("pointermove", onPointerMove, opts);
      contentWindow.removeEventListener("pointerup", onPointerUp, opts);
      contentWindow.removeEventListener("pointerup", onPrimaryUpForDbl, opts);
      contentWindow.removeEventListener("wheel", onWheel, { capture: true });
      contentWindow.removeEventListener("dblclick", onDblClick, opts);
      contentWindow.removeEventListener("keydown", onKeyDown, opts);
      contentWindow.removeEventListener("blur", resetGesture, opts);
    } catch {
    }
    if (typeof window !== "undefined") {
      window.removeEventListener("pointerup", hostUp, true);
      window.removeEventListener("blur", hostUp, true);
    }
    detached = true;
    pendingDelta = null;
    resetGesture();
  };
}
