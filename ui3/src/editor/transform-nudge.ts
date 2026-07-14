export interface Vec3 {
  x: number;
  y: number;
  z: number;
}
export interface Quat {
  x: number;
  y: number;
  z: number;
  w: number;
}

const DEG2RAD = Math.PI / 180;
const RAD2DEG = 180 / Math.PI;

export function tidy(n: number): number {
  return Math.round(n * 1e6) / 1e6;
}

export function nudgeFromKey(
  value: number,
  key: string,
  shiftKey: boolean,
): number | null {
  const step = shiftKey ? 0.01 : 1;
  if (key === "ArrowUp") return tidy(value + step);
  if (key === "ArrowDown") return tidy(value - step);
  return null;
}

function wrap(value: number, length: number): number {
  return value - Math.floor(value / length) * length;
}

export function quatToEulerDeg(q: Quat): Vec3 {
  const out: Vec3 = { x: 0, y: 0, z: 0 };
  const unit = q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w;
  const test = q.x * q.w - q.y * q.z;
  if (test > 0.4995 * unit) {
    out.x = Math.PI / 2;
    out.y = 2 * Math.atan2(q.y, q.x);
    out.z = 0;
  } else if (test < -0.4995 * unit) {
    out.x = -Math.PI / 2;
    out.y = -2 * Math.atan2(q.y, q.x);
    out.z = 0;
  } else {
    out.x = Math.asin(2 * (q.w * q.x - q.y * q.z));
    out.y = Math.atan2(2 * q.w * q.y + 2 * q.z * q.x, 1 - 2 * (q.x * q.x + q.y * q.y));
    out.z = Math.atan2(2 * q.w * q.z + 2 * q.x * q.y, 1 - 2 * (q.z * q.z + q.x * q.x));
  }
  out.x = wrap(out.x * RAD2DEG, 360);
  out.y = wrap(out.y * RAD2DEG, 360);
  out.z = wrap(out.z * RAD2DEG, 360);
  return { x: tidy(out.x), y: tidy(out.y), z: tidy(out.z) };
}

export function eulerDegToQuat(e: Vec3): Quat {
  const pitch = e.x * DEG2RAD;
  const yaw = e.y * DEG2RAD;
  const roll = e.z * DEG2RAD;
  const hp = pitch * 0.5;
  const hy = yaw * 0.5;
  const hr = roll * 0.5;
  const c1 = Math.cos(hp);
  const c2 = Math.cos(hy);
  const c3 = Math.cos(hr);
  const s1 = Math.sin(hp);
  const s2 = Math.sin(hy);
  const s3 = Math.sin(hr);
  return {
    x: c2 * s1 * c3 + s2 * c1 * s3,
    y: s2 * c1 * c3 - c2 * s1 * s3,
    z: c2 * c1 * s3 - s2 * s1 * c3,
    w: c2 * c1 * c3 + s2 * s1 * s3,
  };
}

export function isQuat(v: unknown): v is Quat {
  return !!v && typeof v === "object" && "w" in (v as Record<string, unknown>);
}
