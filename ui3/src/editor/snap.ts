import { eulerDegToQuat, isQuat, quatToEulerDeg, tidy } from "./transform-nudge";
import type { EditorTransform, EditorVec } from "./types";

export interface SnapState {
  on: boolean;
  step: number;
  angle: number;
}

export const SNAP_STEPS = [0.25, 0.5, 1, 0.1] as const;
export const SNAP_ANGLES = [15, 45, 90, 1] as const;

export const DEFAULT_SNAP: SnapState = { on: true, step: 0.25, angle: 15 };

const KEY = "dcl-editor:snap";

export function loadSnap(): SnapState {
  try {
    const raw = globalThis.localStorage?.getItem(KEY);
    if (!raw) return { ...DEFAULT_SNAP };
    const v = JSON.parse(raw) as Partial<SnapState>;
    return {
      on: typeof v.on === "boolean" ? v.on : DEFAULT_SNAP.on,
      step: Number.isFinite(v.step) && Number(v.step) > 0 ? Number(v.step) : DEFAULT_SNAP.step,
      angle: Number.isFinite(v.angle) && Number(v.angle) > 0 ? Number(v.angle) : DEFAULT_SNAP.angle,
    };
  } catch {
    return { ...DEFAULT_SNAP };
  }
}

export function saveSnap(s: SnapState): SnapState {
  try {
    globalThis.localStorage?.setItem(KEY, JSON.stringify(s));
  } catch {
  }
  return s;
}

export function nextIn(list: readonly number[], value: number): number {
  const i = list.indexOf(value);
  return list[(i + 1) % list.length] ?? list[0] ?? value;
}

function quantize(value: number, grid: number): number {
  if (!(grid > 0) || !Number.isFinite(value)) return value;
  return tidy(Math.round(value / grid) * grid);
}

function quantizeVec(v: EditorVec | undefined, grid: number): EditorVec | undefined {
  if (!v) return v;
  return {
    ...v,
    x: quantize(Number(v.x) || 0, grid),
    y: quantize(Number(v.y) || 0, grid),
    z: quantize(Number(v.z) || 0, grid),
  };
}

function same(a: EditorVec | undefined, b: EditorVec | undefined): boolean {
  if (!a || !b) return false;
  return a.x === b.x && a.y === b.y && a.z === b.z && a.w === b.w;
}

// The engine's gizmo emits free-float transforms and has no snap concept, so
// the grid is applied once on release. Anything finer than this is Rust work.
//
// `prev` is what keeps a translate from silently re-rounding a rotation the
// drag never touched: a field that did not move is left exactly as authored.
export function quantizeTransform(
  t: EditorTransform,
  snap: SnapState,
  prev?: EditorTransform | null,
): EditorTransform {
  if (!snap.on) return t;
  const out: EditorTransform = { ...t };
  if (!same(t.position, prev?.position)) {
    const pos = quantizeVec(t.position, snap.step);
    if (pos) out.position = pos;
  }
  const rot = t.rotation;
  if (isQuat(rot) && !same(t.rotation, prev?.rotation)) {
    const euler = quatToEulerDeg(rot);
    out.rotation = eulerDegToQuat({
      x: quantize(euler.x, snap.angle),
      y: quantize(euler.y, snap.angle),
      z: quantize(euler.z, snap.angle),
    });
  }
  return out;
}
