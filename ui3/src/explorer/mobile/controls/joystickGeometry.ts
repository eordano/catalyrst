import type { MovementCode } from "./engineInput";

export const DESIGN_WIDTH = 1600;
export const DESIGN_HEIGHT = 720;

export const CLAMP_RADIUS = 75;
export const FIXED_BASE_RADIUS = 25;
export const RESTING_BASE_X = 160;
export const RESTING_BASE_Y = 165;
export const REVEAL_DELAY_MS = 250;
export const SPRINT_MAGNITUDE = 0.95;
export const SPRINT_DWELL_MS = 500;
export const ACTIVE_AREA_FRACTION = 0.352;
export const ACTIVE_AREA_OFFSET = 171.458;

export const MOVEMENT_DEADZONE = 0.5;
export const AXIS_THRESHOLD = Math.sin(Math.PI / 8);

export const CHAIN_LINKS = 6;

export type Vec2 = { x: number; y: number };

export type JoystickMode = "dynamic" | "fixed";

export type JoystickResolution = {
  output: Vec2;
  magnitude: number;
  keys: MovementCode[];
  sprintEligible: boolean;
};

export function designScale(width: number, height: number): number {
  const portrait = height > width;
  const baseWidth = portrait ? DESIGN_HEIGHT : DESIGN_WIDTH;
  const baseHeight = portrait ? DESIGN_WIDTH : DESIGN_HEIGHT;
  if (baseWidth <= 0 || baseHeight <= 0) return 1;
  return Math.min(width / baseWidth, height / baseHeight);
}

export function activeAreaWidth(viewportWidth: number, scale: number): number {
  return ACTIVE_AREA_FRACTION * viewportWidth + ACTIVE_AREA_OFFSET * scale;
}

export function restingBase(height: number, scale: number): Vec2 {
  return { x: RESTING_BASE_X * scale, y: height - RESTING_BASE_Y * scale };
}

export function movementKeys(
  output: Vec2,
  deadzone: number,
  axisThreshold: number,
): MovementCode[] {
  const magnitude = Math.hypot(output.x, output.y);
  if (magnitude <= deadzone) return [];
  const dx = output.x / magnitude;
  const dy = output.y / magnitude;
  const keys: MovementCode[] = [];
  if (dy < -axisThreshold) keys.push("KeyW");
  if (dy > axisThreshold) keys.push("KeyS");
  if (dx < -axisThreshold) keys.push("KeyA");
  if (dx > axisThreshold) keys.push("KeyD");
  return keys;
}

export function resolveJoystick(
  vector: Vec2,
  radiusPx: number,
  deadzone: number,
  axisThreshold: number,
): JoystickResolution {
  const radius = radiusPx > 0 ? radiusPx : 1;
  const output = { x: vector.x / radius, y: vector.y / radius };
  const magnitude = Math.hypot(output.x, output.y);
  return {
    output,
    magnitude,
    keys: movementKeys(output, deadzone, axisThreshold),
    sprintEligible: magnitude >= SPRINT_MAGNITUDE,
  };
}

export function chainLink(index: number, tipDistance: number, maxChain: number) {
  const reach = Math.max(
    0,
    tipDistance - (index + 1) / (CHAIN_LINKS + 1) - (35 / maxChain) * 0.6,
  );
  const t = Math.min(1, reach / 0.9);
  return { radius: 5 + (14 - 5) * t, alpha: (0.25 + (0.8 - 0.25) * t) * 0.3, reach };
}

export function chainLength(width: number, height: number): number {
  return 0.25 * Math.hypot(width, height);
}
