import type {
  BusEnvelope,
  CameraMode,
  PageToSceneMessage,
  SceneToPageMessage,
  EditorTool,
} from "./bus-protocol";
import { EDITOR_BUS_CHANNEL } from "./bus-protocol";
import { RPC_TIMEOUT_MS, EXPORT_COMPOSITE_TIMEOUT_MS } from "./editor-config";

export { EDITOR_BUS_CHANNEL };

export type EditorCamMode = "none" | "free" | "target";

export interface CameraInputDelta {
  orbitYaw?: number;
  orbitPitch?: number;
  panX?: number;
  panY?: number;
  dolly?: number;
}

export interface CameraSettingsInput {
  preset?: string;
  sensitivity?: { orbit?: number; pan?: number; zoom?: number };
  invertY?: boolean;
}

interface PendingRpc {
  resolve: (value: unknown) => void;
  reject: (reason: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

export interface EditorBus {
  ok: boolean;
  init(): void;
  setTool(tool: EditorTool): void;
  setSelection(selected: string[], active: string | null): void;
  setComponent(entity: string | number | null | undefined, name: string, json: string): void;
  addComponent(entity: string | number | null | undefined, name: string): void;
  deleteComponent(entity: string | number | null | undefined, name: string): void;
  deleteEntity(entity: string | number | null | undefined, recursive?: boolean): void;
  setCamMode(mode: EditorCamMode): void;
  setCameraInput(d: CameraInputDelta | null | undefined): void;
  setCameraSettings(s: CameraSettingsInput | null | undefined): void;
  focus(entity: string | number | null | undefined, orbit?: boolean): void;
  orientAxis(mode: EditorCamMode, axis: string): void;
  toggleOrtho(): void;
  addEntity(name?: string, parent?: number, components?: Record<string, unknown> | null): void;
  loadScene(composite: string, replace?: boolean): void;
  announcePlayState(playing: boolean, paused: boolean): void;
  rpc(method: string, args?: unknown[], timeoutMs?: number): Promise<unknown>;
  exportComposite(timeoutMs?: number): Promise<unknown>;
  onMessage(cb: (msg: SceneToPageMessage) => void): () => void;
  close(): void;
}

const NULL_BUS: EditorBus = {
  ok: false,
  init() {},
  setTool() {},
  setSelection() {},
  setComponent() {},
  addComponent() {},
  deleteComponent() {},
  deleteEntity() {},
  addEntity() {},
  setCamMode() {},
  setCameraInput() {},
  setCameraSettings() {},
  focus() {},
  orientAxis() {},
  toggleOrtho() {},
  loadScene() {},
  announcePlayState() {},
  rpc() {
    return Promise.reject(new Error("editor bus unavailable"));
  },
  exportComposite() {
    return Promise.reject(new Error("editor bus unavailable"));
  },
  onMessage() {
    return () => {};
  },
  close() {},
};

const EDITOR_TOOLS = new Set<EditorTool>(["select", "translate", "rotate", "scale"]);
const EDITOR_CAM_MODES = new Set<EditorCamMode>(["none", "free", "target"]);

const toWireCamMode = (mode: EditorCamMode): CameraMode => (mode === "none" ? "off" : mode);

export function createEditorBus(): EditorBus {
  if (typeof BroadcastChannel === "undefined") return NULL_BUS;

  let channel: BroadcastChannel;
  try {
    channel = new BroadcastChannel(EDITOR_BUS_CHANNEL);
  } catch {
    return NULL_BUS;
  }

  const listeners = new Set<(msg: SceneToPageMessage) => void>();
  const pending = new Map<number | string, PendingRpc>();
  const token = Math.random().toString(36).slice(2, 10);
  let rpcSeq = 0;

  channel.onmessage = (ev: MessageEvent) => {
    const env = (ev?.data ?? null) as BusEnvelope | null;
    if (!env || typeof env !== "object" || env.to !== "page" || !env.msg) return;
    const msg = env.msg;
    if (msg.type === "rpc-reply" && pending.has(msg.id)) {
      const entry = pending.get(msg.id);
      if (entry) {
        pending.delete(msg.id);
        if (entry.timer) clearTimeout(entry.timer);
        if (msg.ok) entry.resolve(msg.result);
        else entry.reject(new Error(msg.error || "rpc failed"));
      }
      return;
    }
    for (const cb of listeners) {
      try {
        cb(msg);
      } catch {
      }
    }
  };

  const post = (msg: PageToSceneMessage) => {
    try {
      channel.postMessage({ to: "scene", msg } satisfies BusEnvelope);
    } catch {
    }
  };

  const bus: EditorBus = {
    ok: true,
    init() {
      post({ type: "init" });
    },
    setTool(tool) {
      if (EDITOR_TOOLS.has(tool)) post({ type: "set-tool", tool });
    },
    setSelection(selected, active) {
      post({
        type: "set-selection",
        selected: Array.isArray(selected) ? selected : [],
        active: active == null ? null : active,
      });
    },
    setComponent(entity, name, json) {
      if (entity == null || !name || typeof json !== "string") return;
      post({ type: "set-component", entity: String(entity), name, json });
    },
    addComponent(entity, name) {
      if (entity == null || !name) return;
      post({ type: "add-component", entity: String(entity), name });
    },
    deleteComponent(entity, name) {
      if (entity == null || !name) return;
      post({ type: "delete-component", entity: String(entity), name });
    },
    deleteEntity(entity, recursive = true) {
      if (entity == null) return;
      post({ type: "entity-deleted", entity: String(entity), recursive: recursive !== false });
    },
    setCamMode(mode) {
      if (EDITOR_CAM_MODES.has(mode)) post({ type: "set-camera", mode: toWireCamMode(mode) });
    },
    setCameraInput(d) {
      if (!d || typeof d !== "object") return;
      const out: Extract<PageToSceneMessage, { type: "camera-input" }> = { type: "camera-input" };
      for (const k of ["orbitYaw", "orbitPitch", "panX", "panY", "dolly"] as const) {
        const v = d[k];
        if (typeof v === "number" && v !== 0) out[k] = v;
      }
      if (Object.keys(out).length > 1) post(out);
    },
    setCameraSettings(s) {
      if (!s || typeof s !== "object") return;
      const sens: { orbit?: number; pan?: number; zoom?: number } =
        s.sensitivity && typeof s.sensitivity === "object" ? s.sensitivity : {};
      post({
        type: "camera-settings",
        preset: s.preset === "maya" || s.preset === "blender-lmb" ? s.preset : "blender",
        sensitivity: {
          orbit: Number(sens.orbit) || 1,
          pan: Number(sens.pan) || 1,
          zoom: Number(sens.zoom) || 1,
        },
        invertY: !!s.invertY,
      });
    },
    focus(entity, orbit = true) {
      if (entity == null) return;
      post({ type: "focus", entity: String(entity), orbit });
    },
    orientAxis(mode, axis) {
      if (EDITOR_CAM_MODES.has(mode) && typeof axis === "string") {
        post({ type: "set-camera", mode: toWireCamMode(mode), axis });
      }
    },
    toggleOrtho() {
      post({ type: "set-camera-projection", ortho: "toggle" });
    },
    addEntity(name, parent = 0, components = null) {
      post({
        type: "add-entity",
        name: String(name || "Entity"),
        parent: Number(parent) || 0,
        components: components && typeof components === "object" ? components : null,
      });
    },
    loadScene(composite, replace = false) {
      if (typeof composite !== "string" || composite.trim() === "") return;
      post({ type: "load-scene", composite, ...(replace ? { replace: true } : {}) });
    },
    announcePlayState(playing, paused) {
      const msg: SceneToPageMessage = { type: "play-state", playing: !!playing, paused: !!paused };
      try {
        channel.postMessage({ to: "page", msg } satisfies BusEnvelope);
      } catch {
      }
    },
    rpc(method, args = [], timeoutMs = RPC_TIMEOUT_MS) {
      return new Promise<unknown>((resolve, reject) => {
        const id = `${token}-${++rpcSeq}`;
        const timer = setTimeout(() => {
          if (pending.has(id)) {
            pending.delete(id);
            reject(new Error("rpc timeout: " + method));
          }
        }, timeoutMs);
        pending.set(id, { resolve, reject, timer });
        post({ type: "rpc", id, method, args });
      });
    },
    exportComposite(timeoutMs = EXPORT_COMPOSITE_TIMEOUT_MS) {
      return bus.rpc("exportComposite", [], timeoutMs);
    },
    onMessage(cb) {
      if (typeof cb !== "function") return () => {};
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    close() {
      listeners.clear();
      for (const { reject, timer } of pending.values()) {
        if (timer) clearTimeout(timer);
        try {
          reject(new Error("editor bus closed"));
        } catch {
        }
      }
      pending.clear();
      channel.onmessage = null;
      try {
        channel.close();
      } catch {
      }
    },
  };

  return bus;
}
