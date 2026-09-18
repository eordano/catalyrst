import { useEffect, useState } from "react";

export type FpsStats = {
  page: number;
  engine: number | null;
  ms: number;
};

type Heartbeat = (this: unknown, fps?: number) => unknown;

type HeartbeatWindow = Window & {
  __engineHeartbeat?: Heartbeat;
};

const SAMPLE_MS = 500;
export const ENGINE_RATE_STALE_MS = 2000;

export function useFps(enabled: boolean): FpsStats {
  const [stats, setStats] = useState<FpsStats>({ page: 0, engine: null, ms: 0 });

  useEffect(() => {
    if (!enabled) return;
    let raf = 0;
    let frames = 0;
    let engineFrames = 0;
    let engineRate: number | null = null;
    let engineRateAt = 0;
    let hooked: HeartbeatWindow | null = null;
    let original: Heartbeat | undefined;
    let wrapper: Heartbeat | undefined;
    let detached = false;
    let windowStart = performance.now();
    let lastTs = windowStart;
    let msAccum = 0;

    const hookEngine = () => {
      const w = window as HeartbeatWindow;
      if (w === hooked || typeof w.__engineHeartbeat !== "function") return;
      original = w.__engineHeartbeat;
      wrapper = function (this: unknown, fps?: number) {
        if (!detached) {
          if (typeof fps === "number") {
            engineRate = fps;
            engineRateAt = performance.now();
          } else {
            engineFrames += 1;
          }
        }
        return original?.call(this, fps);
      };
      w.__engineHeartbeat = wrapper;
      hooked = w;
    };

    const readEngine = (now: number, secs: number): number | null => {
      if (engineRate !== null) {
        return now - engineRateAt <= ENGINE_RATE_STALE_MS ? Math.round(engineRate) : 0;
      }
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
          engine: readEngine(t, secs),
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
      detached = true;
      if (hooked && original && hooked.__engineHeartbeat === wrapper) {
        hooked.__engineHeartbeat = original;
      }
    };
  }, [enabled]);

  return stats;
}
