import type { Scene } from "./api";
export function worldName(value: string): string | null {
  let name = value.trim();
  try {
    name = new URL(name).searchParams.get("realm") || "";
  } catch {
    /* A bare world name is valid. */
  }
  return name.length <= 253 &&
    name.includes(".") &&
    name.split(".").every((p) => /^[a-z\d](?:[a-z\d-]{0,61}[a-z\d])?$/i.test(p))
    ? name.toLowerCase()
    : null;
}
export const destinationKey = (scene: Scene) =>
  scene.world || `${scene.x},${scene.y}`;
export const destinationLabel = (scene: Scene) =>
  scene.world || `${scene.x}, ${scene.y}`;
export const destinationUrl = (scene: Scene) =>
  `https://decentraland.org/jump/?${scene.world ? `realm=${encodeURIComponent(scene.world)}` : `position=${scene.x},${scene.y}`}`;
