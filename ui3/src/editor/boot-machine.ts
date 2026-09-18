type BootPhase =
  | "idle"
  | "booting"
  | "handshaking"
  | "ready"
  | "error";

export type BootStage = "download" | "compile" | "init" | "workers" | "gpu";

export interface BootState {
  phase: BootPhase;
  progress: number | null;
  stage: BootStage | null;
  engineReady: boolean;
  sceneReady: boolean;
  reason: string | null;
}

export const INITIAL_BOOT: BootState = {
  phase: "idle",
  progress: null,
  stage: null,
  engineReady: false,
  sceneReady: false,
  reason: null,
};

export type BootEvent =
  | { type: "viewport"; src: string | null }
  | { type: "iframe-load" }
  | { type: "progress"; pct: number; stage?: BootStage | null }
  | { type: "engine-ready" }
  | { type: "scene-ready" }
  | { type: "bus-reset" }
  | { type: "engine-error"; reason?: string }
  | { type: "timeout" }
  | { type: "retry" };

const clampPct = (n: number): number =>
  !isFinite(n) ? 0 : Math.max(0, Math.min(100, Math.round(n)));

function readyPhase(next: BootState): BootState {
  return { ...next, phase: "ready", reason: null };
}

export function bootReducer(state: BootState, event: BootEvent): BootState {
  switch (event.type) {
    case "viewport": {
      if (!event.src) return { ...INITIAL_BOOT };
      return { ...INITIAL_BOOT, phase: "booting" };
    }

    case "scene-ready":
      if (state.phase === "idle") return state;
      if (state.phase === "error") {
        return state.engineReady
          ? readyPhase({ ...state, sceneReady: true })
          : { ...state, sceneReady: true };
      }
      return state.engineReady
        ? readyPhase({ ...state, sceneReady: true })
        : { ...state, sceneReady: true, phase: "handshaking" };

    case "engine-ready": {
      if (state.phase === "idle") return { ...state, engineReady: true };
      if (state.phase === "error") return { ...state, engineReady: true };
      return { ...state, engineReady: true, phase: state.sceneReady ? "ready" : "handshaking" };
    }

    case "progress": {
      const pct = clampPct(event.pct);
      const stage = event.stage ?? stageFromProgress(pct);
      if (state.phase === "idle") return { ...state, progress: pct, stage };
      if (pct >= 100) {
        if (state.phase === "booting")
          return { ...state, progress: pct, stage, phase: "handshaking" };
        return { ...state, progress: pct, stage };
      }
      return { ...state, progress: pct, stage };
    }

    case "iframe-load":
      if (state.phase === "idle") return state;
      return state;

    case "bus-reset":
      if (state.phase === "idle") return state;
      return { ...state, sceneReady: false, phase: "handshaking" };

    case "engine-error":
      if (state.phase === "idle") return state;
      return { ...state, phase: "error", reason: event.reason ?? "engine error" };

    case "timeout":
      if (state.phase === "ready" || state.phase === "idle") return state;
      return { ...state, phase: "error", reason: "taking longer than usual" };

    case "retry":
      if (state.phase === "idle") return state;
      return { ...INITIAL_BOOT, phase: "booting" };

    default:
      return state;
  }
}

function stageFromProgress(pct: number): BootStage {
  if (pct < 80) return "download";
  if (pct < 85) return "compile";
  if (pct < 90) return "init";
  if (pct < 95) return "workers";
  return "gpu";
}

const STAGE_TEXT: Record<BootStage, string> = {
  download: "Downloading engine",
  compile: "Compiling engine",
  init: "Starting engine",
  workers: "Starting workers",
  gpu: "Preparing GPU pipelines",
};

interface BootOverlay {
  show: boolean;
  kind: "loading" | "error" | null;
  text: string;
}

export function bootOverlay(state: BootState): BootOverlay {
  switch (state.phase) {
    case "ready":
    case "idle":
      return { show: false, kind: null, text: "" };
    case "error":
      return {
        show: true,
        kind: "error",
        text: state.reason && state.reason !== "taking longer than usual"
          ? state.reason
          : state.engineReady
          ? "The engine started, but the scene editor did not connect."
          : "The 3D engine is taking longer than usual to start.",
      };
    case "handshaking":
      return { show: true, kind: "loading", text: "Starting scene\u{2026}" };
    case "booting":
    default: {
      if (state.progress == null)
        return { show: true, kind: "loading", text: "Loading scene editor\u{2026}" };
      if (state.progress >= 100)
        return { show: true, kind: "loading", text: "Starting scene\u{2026}" };
      const stage = state.stage ?? stageFromProgress(state.progress);
      return {
        show: true,
        kind: "loading",
        text: `${STAGE_TEXT[stage]}\u{2026} ${state.progress}%`,
      };
    }
  }
}

export const isEditorReady = (s: BootState): boolean => s.phase === "ready";
export const isLiveEditing = (s: BootState): boolean => s.sceneReady;
