// GENERATED from catalyrst/ui3/src/generated/editor-bus.ts by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { BusEnvelope, CameraMode, CameraPreset, CameraSensitivity, EditorEntityNode, EditorTool, LiveSceneInfo, NodeDisplay, OrthoRequest, PageToSceneMessage, ParcelCoord, RpcId, SceneToPageMessage } from "./editor-bus";

export const CameraModeSchema = z.enum(["off", "free", "target"]);

export const CameraPresetSchema = z.enum(["blender", "blender-lmb", "maya"]);

export const CameraSensitivitySchema = z.object({
  orbit: z.number(),
  pan: z.number(),
  zoom: z.number(),
});

export const EditorToolSchema = z.enum(["select", "translate", "rotate", "scale"]);

export const NodeDisplaySchema = z.enum(["always", "selected", "selecting"]);

export const OrthoRequestSchema = z.enum(["toggle", "ortho", "perspective"]);

export const RpcIdSchema = z.union([z.number(), z.string()]);

export const PageToSceneMessageSchema = z.union([z.object({
  type: z.literal("init"),
}), z.object({
  type: z.literal("set-tool"),
  tool: EditorToolSchema,
}), z.object({
  type: z.literal("set-flags"),
  orientGlobal: z.boolean().optional(),
  pivotEach: z.boolean().optional(),
  nodeDisplay: NodeDisplaySchema.optional(),
  showLinks: z.boolean().optional(),
}), z.object({
  type: z.literal("set-selection"),
  selected: z.array(z.string()),
  active: z.string().nullable(),
}), z.object({
  type: z.literal("set-camera"),
  mode: CameraModeSchema,
  axis: z.string().optional(),
}), z.object({
  type: z.literal("focus"),
  entity: z.string(),
  orbit: z.boolean().optional(),
}), z.object({
  type: z.literal("refresh"),
}), z.object({
  type: z.literal("pointer-up"),
}), z.object({
  type: z.literal("fly-speed"),
  factor: z.number(),
}), z.object({
  type: z.literal("camera-input"),
  orbitYaw: z.number().optional(),
  orbitPitch: z.number().optional(),
  panX: z.number().optional(),
  panY: z.number().optional(),
  dolly: z.number().optional(),
}), z.object({
  type: z.literal("camera-settings"),
  preset: CameraPresetSchema,
  sensitivity: CameraSensitivitySchema,
  invertY: z.boolean(),
}), z.object({
  type: z.literal("set-camera-projection"),
  ortho: OrthoRequestSchema,
}), z.object({
  type: z.literal("resync"),
}), z.object({
  type: z.literal("component-written"),
  entity: z.string(),
  name: z.string(),
  json: z.string(),
}), z.object({
  type: z.literal("set-component"),
  entity: z.string(),
  name: z.string(),
  json: z.string(),
}), z.object({
  type: z.literal("add-component"),
  entity: z.string(),
  name: z.string(),
}), z.object({
  type: z.literal("delete-component"),
  entity: z.string(),
  name: z.string(),
}), z.object({
  type: z.literal("load-scene"),
  composite: z.string(),
  replace: z.boolean().optional(),
}), z.object({
  type: z.literal("add-entity"),
  name: z.string(),
  parent: z.number(),
  components: z.record(z.string(), z.unknown()).nullable().optional(),
}), z.object({
  type: z.literal("entity-deleted"),
  entity: z.string(),
  recursive: z.boolean(),
}), z.object({
  type: z.literal("rpc"),
  id: RpcIdSchema,
  method: z.string(),
  args: z.array(z.unknown()).optional(),
})]);

export const EditorEntityNodeSchema = z.object({
  id: z.string(),
  name: z.string().nullable(),
  parent: z.string(),
});

export const ParcelCoordSchema = z.object({
  x: z.number(),
  y: z.number(),
});

export const LiveSceneInfoSchema = z.object({
  hash: z.string(),
  base_url: z.string().optional(),
  title: z.string(),
  parcels: z.array(ParcelCoordSchema),
  isPortable: z.boolean(),
  isBroken: z.boolean(),
  isBlocked: z.boolean(),
  isSuper: z.boolean(),
  sdkVersion: z.string(),
});

export const SceneToPageMessageSchema = z.union([z.object({
  type: z.literal("scene-ready"),
  bridge: z.number().optional(),
  scene: LiveSceneInfoSchema.nullable(),
  frozen: z.boolean(),
  tool: EditorToolSchema,
  orientGlobal: z.boolean(),
  pivotEach: z.boolean(),
  selected: z.array(z.string()),
  active: z.string().nullable(),
}), z.object({
  type: z.literal("selection"),
  selected: z.array(z.string()),
  active: z.string().nullable(),
  components: z.record(z.string(), z.record(z.string(), z.unknown())).optional(),
}), z.object({
  type: z.literal("entities"),
  entities: z.array(EditorEntityNodeSchema),
}), z.object({
  type: z.literal("drag-start"),
}), z.object({
  type: z.literal("drag-end"),
  transforms: z.record(z.string(), z.unknown()),
}), z.object({
  type: z.literal("tool"),
  tool: EditorToolSchema,
}), z.object({
  type: z.literal("rpc-reply"),
  id: RpcIdSchema,
  ok: z.boolean(),
  result: z.unknown().optional(),
  error: z.string().optional(),
}), z.object({
  type: z.literal("play-state"),
  playing: z.boolean(),
  paused: z.boolean(),
}), z.object({
  type: z.literal("camera-pose"),
  x: z.number(),
  y: z.number(),
  z: z.number(),
  yaw: z.number(),
  pitch: z.number(),
})]);

export const BusEnvelopeSchema = z.union([z.object({
  to: z.literal("scene"),
  msg: PageToSceneMessageSchema,
}), z.object({
  to: z.literal("page"),
  msg: SceneToPageMessageSchema,
})]);

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertBusEnvelope = Assert<Mutual<BusEnvelope, z.infer<typeof BusEnvelopeSchema>>>;
export type _AssertCameraMode = Assert<Mutual<CameraMode, z.infer<typeof CameraModeSchema>>>;
export type _AssertCameraPreset = Assert<Mutual<CameraPreset, z.infer<typeof CameraPresetSchema>>>;
export type _AssertCameraSensitivity = Assert<Mutual<CameraSensitivity, z.infer<typeof CameraSensitivitySchema>>>;
export type _AssertEditorEntityNode = Assert<Mutual<EditorEntityNode, z.infer<typeof EditorEntityNodeSchema>>>;
export type _AssertEditorTool = Assert<Mutual<EditorTool, z.infer<typeof EditorToolSchema>>>;
export type _AssertLiveSceneInfo = Assert<Mutual<LiveSceneInfo, z.infer<typeof LiveSceneInfoSchema>>>;
export type _AssertNodeDisplay = Assert<Mutual<NodeDisplay, z.infer<typeof NodeDisplaySchema>>>;
export type _AssertOrthoRequest = Assert<Mutual<OrthoRequest, z.infer<typeof OrthoRequestSchema>>>;
export type _AssertPageToSceneMessage = Assert<Mutual<PageToSceneMessage, z.infer<typeof PageToSceneMessageSchema>>>;
export type _AssertParcelCoord = Assert<Mutual<ParcelCoord, z.infer<typeof ParcelCoordSchema>>>;
export type _AssertRpcId = Assert<Mutual<RpcId, z.infer<typeof RpcIdSchema>>>;
export type _AssertSceneToPageMessage = Assert<Mutual<SceneToPageMessage, z.infer<typeof SceneToPageMessageSchema>>>;
