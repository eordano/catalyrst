import { worldName } from "./destinations";
import type { Scene } from "./api";
export type LiveLocation = Scene & { updatedAt: number };
export const LOCATION_TOPIC = "dcl.social.location.v1";
export function freshLocation(
  value: unknown,
  now = Date.now(),
): LiveLocation | null {
  if (!value || typeof value !== "object") return null;
  const v = value as Partial<LiveLocation>;
  if (
    !Number.isInteger(v.x) ||
    !Number.isInteger(v.y) ||
    Math.abs(v.x!) > 150 ||
    Math.abs(v.y!) > 150 ||
    typeof v.updatedAt !== "number" ||
    !Number.isFinite(v.updatedAt) ||
    now - v.updatedAt >= 45000 ||
    v.updatedAt - now > 5000
  )
    return null;
  if (
    v.world !== undefined &&
    (typeof v.world !== "string" || !worldName(v.world))
  )
    return null;
  return {
    x: v.x!,
    y: v.y!,
    ...(v.world ? { world: worldName(v.world)! } : {}),
    updatedAt: v.updatedAt,
  };
}
