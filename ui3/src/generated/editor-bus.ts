// GENERATED from bridge_protocol::editor via ts-rs + catalyrst/sites/scripts/gen-ts-types.sh. Do not edit.

export type BusEnvelope = { "to": "scene", msg: PageToSceneMessage, } | { "to": "page", msg: SceneToPageMessage, };

export type CameraMode = "off" | "free" | "target";

export type CameraPreset = "blender" | "blender-lmb" | "maya";

export type CameraSensitivity = { orbit: number, pan: number, zoom: number, };

export type EditorEntityNode = { id: string, name: string | null, parent: string, };

export type EditorTool = "select" | "translate" | "rotate" | "scale";

export type LiveSceneInfo = { hash: string, base_url?: string, title: string, parcels: Array<ParcelCoord>, isPortable: boolean, isBroken: boolean, isBlocked: boolean, isSuper: boolean, sdkVersion: string, };

export type NodeDisplay = "always" | "selected" | "selecting";

export type OrthoRequest = "toggle" | "ortho" | "perspective";

export type PageToSceneMessage = { "type": "init" } | { "type": "set-tool", tool: EditorTool, } | { "type": "set-flags", orientGlobal?: boolean, pivotEach?: boolean, nodeDisplay?: NodeDisplay, showLinks?: boolean, } | { "type": "set-selection", selected: Array<string>, active: string | null, } | { "type": "set-camera", mode: CameraMode, axis?: string, } | { "type": "focus", entity: string, orbit?: boolean, } | { "type": "refresh" } | { "type": "pointer-up" } | { "type": "fly-speed", factor: number, } | { "type": "camera-input", orbitYaw?: number, orbitPitch?: number, panX?: number, panY?: number, dolly?: number, } | { "type": "camera-settings", preset: CameraPreset, sensitivity: CameraSensitivity, invertY: boolean, } | { "type": "set-camera-projection", ortho: OrthoRequest, } | { "type": "resync" } | { "type": "component-written", entity: string, name: string, json: string, } | { "type": "set-component", entity: string, name: string, json: string, } | { "type": "add-component", entity: string, name: string, } | { "type": "delete-component", entity: string, name: string, } | { "type": "load-scene", composite: string, replace?: boolean, } | { "type": "add-entity", name: string, parent: number, components?: Record<string, unknown> | null, } | { "type": "entity-deleted", entity: string, recursive: boolean, } | { "type": "rpc", id: RpcId, method: string, args?: Array<unknown>, };

export type ParcelCoord = { x: number, y: number, };

export type RpcId = number | string;

export type SceneToPageMessage = { "type": "scene-ready", bridge?: number, scene: LiveSceneInfo | null, frozen: boolean, tool: EditorTool, orientGlobal: boolean, pivotEach: boolean, selected: Array<string>, active: string | null, } | { "type": "selection", selected: Array<string>, active: string | null, components?: Record<string, Record<string, unknown>>, } | { "type": "entities", entities: Array<EditorEntityNode>, } | { "type": "drag-start" } | { "type": "drag-end", transforms: Record<string, unknown>, } | { "type": "tool", tool: EditorTool, } | { "type": "rpc-reply", id: RpcId, ok: boolean, result?: unknown, error?: string, } | { "type": "play-state", playing: boolean, paused: boolean, } | { "type": "camera-pose", x: number, y: number, z: number, yaw: number, pitch: number, };

export const EDITOR_BUS_CHANNEL = "dcl-editor-bus";
export const SCENE_BRIDGE_VERSION = 8;
