
const STORAGE_KEY = "dcl-editor:camera-prefs";
const PRESETS = new Set(["blender", "blender-lmb", "maya"]);

export const DEFAULT_CAMERA_PREFS = Object.freeze({
  preset: "blender",
  sensitivity: Object.freeze({ orbit: 1, pan: 1, zoom: 1 }),
  invertY: false,
});

function clampSens(n) {
  const v = typeof n === "number" ? n : parseFloat(n);
  if (!Number.isFinite(v)) return 1;
  return Math.max(0.1, Math.min(3, v));
}

export function normalizeCameraPrefs(raw) {
  const r = raw && typeof raw === "object" ? raw : {};
  const s = r.sensitivity && typeof r.sensitivity === "object" ? r.sensitivity : {};
  return {
    preset: PRESETS.has(r.preset) ? r.preset : "blender",
    sensitivity: {
      orbit: clampSens(s.orbit),
      pan: clampSens(s.pan),
      zoom: clampSens(s.zoom),
    },
    invertY: !!r.invertY,
  };
}

export function loadCameraPrefs() {
  try {
    if (typeof localStorage === "undefined") return normalizeCameraPrefs(null);
    const txt = localStorage.getItem(STORAGE_KEY);
    if (!txt) return normalizeCameraPrefs(null);
    return normalizeCameraPrefs(JSON.parse(txt));
  } catch {
    return normalizeCameraPrefs(null);
  }
}

export function saveCameraPrefs(prefs) {
  const norm = normalizeCameraPrefs(prefs);
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(norm));
    }
  } catch {
  }
  return norm;
}
