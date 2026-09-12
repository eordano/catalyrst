import { useEffect, useState } from "react";

export type FpsStats = {
  page: number;
  engine: number | null;
  ms: number;
};

type HeartbeatWindow = Window & {
  __engineHeartbeat?: (...a: unknown[]) => unknown;
  __nativeEngineFps?: number;
};

const SAMPLE_MS = 500;

export function useFps(enabled: boolean): FpsStats {
  const [stats, setStats] = useState<FpsStats>({ page: 0, engine: null, ms: 0 });

  useEffect(() => {
    if (!enabled) return;
    let raf = 0;
    let frames = 0;
    let engineFrames = 0;
    let hooked: HeartbeatWindow | null = null;
    let original: ((...a: unknown[]) => unknown) | undefined;
    let windowStart = performance.now();
    let lastTs = windowStart;
    let msAccum = 0;

    const hookEngine = () => {
      const w = window as HeartbeatWindow;
      if (w === hooked || typeof w.__engineHeartbeat !== "function") return;
      original = w.__engineHeartbeat;
      w.__engineHeartbeat = function (this: unknown, ...args: unknown[]) {
        engineFrames += 1;
        return original?.apply(this, args);
      };
      hooked = w;
    };

    const readEngine = (secs: number): number | null => {
      const native = (window as HeartbeatWindow).__nativeEngineFps;
      if (typeof native === "number") return Math.round(native);
      return hooked ? Math.round(engineFrames / secs) : null;
    };

    const loop = (t: number) => {
      frames += 1;
      msAccum += t - lastTs;
      lastTs = t;
      hookEngine();
      if (t - windowStart >= SAMPLE_MS) {
        const secs = (t - windowStart) / 1000;
        setStats({
          page: Math.round(frames / secs),
          engine: readEngine(secs),
          ms: Number((msAccum / frames).toFixed(1)),
        });
        frames = 0;
        engineFrames = 0;
        msAccum = 0;
        windowStart = t;
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);

    return () => {
      cancelAnimationFrame(raf);
      if (hooked && original) hooked.__engineHeartbeat = original;
    };
  }, [enabled]);

  return stats;
}
