import { lazy } from "react";

export type { MobileEnv, MobileOrientation, MobileOverride } from "./detect";
export {
  MOBILE_HINTED_MAX_SHORT_EDGE,
  MOBILE_MAX_SHORT_EDGE,
  MOBILE_OVERRIDE_PARAM,
  MOBILE_OVERRIDE_STORAGE_KEY,
  SSR_VIEWPORT_HEIGHT,
  SSR_VIEWPORT_WIDTH,
  getMobileEnv,
  getServerMobileEnv,
  refreshMobileEnv,
  setMobileOverride,
  subscribeMobileEnv,
  useIsMobile,
  useMobileEnv,
} from "./detect";

export type { WorldKeyCode } from "./keys";
export {
  WORLD_CANVAS_ID,
  WORLD_KEY_CODES,
  getWorldCanvas,
  isSynthesizedWorldKey,
  isWorldKeyCode,
} from "./keys";

export * from "./layout";

export type {
  MobileAction,
  MobileGlyph,
  MobileSheetSnap,
  MobileTab,
  MobileTabGlyph,
} from "./chrome/types";
export type { MobileHudFrameProps } from "./chrome/MobileHudFrame";

export type {
  ActionCode,
  CameraCode,
  EngineInput,
  EngineInputConfig,
  EngineInputLogEntry,
  EngineInputOptions,
  LookStrategy,
  ModifierCode,
  MovementCode,
  SyntheticKeyCode,
} from "./controls/engineInput";
export { createEngineInput } from "./controls/engineInput";

export type { JoystickMode, JoystickResolution, Vec2 } from "./controls/joystickGeometry";
export {
  AXIS_THRESHOLD,
  CLAMP_RADIUS,
  MOVEMENT_DEADZONE,
  SPRINT_DWELL_MS,
  SPRINT_MAGNITUDE,
  activeAreaWidth,
  designScale,
  movementKeys,
  resolveJoystick,
  restingBase,
} from "./controls/joystickGeometry";

export type { TouchControlsProps } from "./controls/TouchControls";
export type { JoystickState, VirtualJoystickProps } from "./controls/VirtualJoystick";
export type { LookSurfaceProps } from "./controls/LookSurface";

export const TouchControls = lazy(() =>
  import("./controls").then((m) => ({ default: m.TouchControls })),
);

export const MobileHudFrame = lazy(() =>
  import("./chrome").then((m) => ({ default: m.MobileHudFrame })),
);
