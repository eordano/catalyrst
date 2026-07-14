export type BootPhase =
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
      if (state.phase === "idle") return { ...INITIAL_BOOT, phase: "booting" };
      return state;
    }

    case "scene-ready":
      if (state.phase === "idle") return state;
      return readyPhase({ ...state, sceneReady: true, engineReady: true });

    case "engine-ready": {
      if (state.phase === "idle") return { ...state, engineReady: true };
      return readyPhase({ ...state, engineReady: true });
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
      if (state.engineReady)
        return { ...state, sceneReady: false };
      return { ...state, sceneReady: false, phase: "handshaking" };

    case "engine-error":
      if (state.phase === "idle") return state;
      if (state.phase === "ready") return state;
      return { ...state, phase: "error", reason: event.reason ?? "engine error" };

    case "timeout":
      if (state.phase === "ready" || state.phase === "idle") return state;
      if (state.engineReady) return readyPhase(state);
      return { ...state, phase: "error", reason: "taking longer than usual" };

    case "retry":
      if (state.phase === "idle") return state;
      return {
        ...state,
        phase: state.engineReady ? "handshaking" : "booting",
        reason: null,
      };

    default:
      return state;
  }
}

export function stageFromProgress(pct: number): BootStage {
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

export interface BootOverlay {
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
        text: "The 3D engine is taking longer than usual to start.",
      };
    case "handshaking":
      return { show: true, kind: "loading", text: "Starting scene…" };
    case "booting":
    default: {
      if (state.progress == null)
        return { show: true, kind: "loading", text: "Loading scene editor…" };
      if (state.progress >= 100)
        return { show: true, kind: "loading", text: "Starting scene…" };
      const stage = state.stage ?? stageFromProgress(state.progress);
      return {
        show: true,
        kind: "loading",
        text: `${STAGE_TEXT[stage]}… ${state.progress}%`,
      };
    }
  }
}

export const isEditorReady = (s: BootState): boolean => s.phase === "ready";
export const isLiveEditing = (s: BootState): boolean => s.sceneReady;
