import { useEffect, useRef, useState } from "react";
import { PROJECT_CACHE, setProjectPlayState } from "../project-cache";

export type ProjectRealmStatus = "pending" | "ready" | "error";

export function useProjectRealm(
  viewportSrc: string | null | undefined,
  prepareRealm: (() => Promise<unknown>) | null | undefined,
): ProjectRealmStatus {
  const isProjectRealm = typeof viewportSrc === "string" && /_project/.test(viewportSrc);
  const [status, setStatus] = useState<ProjectRealmStatus>(() =>
    isProjectRealm ? "pending" : "ready",
  );
  const prepareRealmRef = useRef(prepareRealm);
  prepareRealmRef.current = prepareRealm;

  useEffect(() => {
    if (!isProjectRealm) return undefined;
    let cancelled = false;
    const sleepMs = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

    const ensureWorker = async () => {
      try {
        const swc = typeof navigator !== "undefined" ? navigator.serviceWorker : null;
        if (!swc || typeof swc.register !== "function") return;
        const reg = await swc.register("/_play/service_worker.js", { scope: "/_play/" });
        for (let i = 0; i < 60 && !reg.active; i += 1) await sleepMs(100);
        for (let i = 0; i < 15 && !swc.controller; i += 1) await sleepMs(100);
      } catch {
      }
    };

    const populate = async () => {
      const fn = prepareRealmRef.current;
      if (typeof fn !== "function") return;
      try {
        await fn();
      } catch {
      }
    };

    const run = async () => {
      const C = typeof caches !== "undefined" ? caches : null;
      const search = typeof window !== "undefined" ? window.location.search : "";
      const isLocalReopen = /[?&]source=local(?:[&=]|$)/.test(search);
      const isDraftReopen = /[?&]draft=/.test(search);
      if (C) {
        const cache = await C.open(PROJECT_CACHE).catch(() => null);
        if (cache) {
          if (!isLocalReopen) {
            if (isDraftReopen) {
              for (let i = 0; i < 20 && !prepareRealmRef.current; i += 1) {
                if (cancelled) return;
                await sleepMs(200);
              }
            }
            // Wipe lazily, only once the prepare fn exists, and retry until the
            // realm is actually seeded: wiping while a slow reopen hydration has
            // not yet provided prepareRealm would strand an empty cache and boot
            // the engine against the base template (the reopen scene-flip).
            let wiped = false;
            for (let attempt = 0; attempt < 5; attempt += 1) {
              if (cancelled) return;
              if (prepareRealmRef.current) {
                if (!wiped) {
                  const keys = await cache.keys();
                  await Promise.all(keys.map((k) => cache.delete(k)));
                  wiped = true;
                }
                await populate();
                if ((await cache.keys()).length > 0) break;
              }
              await sleepMs(300);
            }
          } else {
            let populated = false;
            for (let i = 0; i < 40; i += 1) {
              if (cancelled) return;
              const keys = await cache.keys();
              if (keys.length > 0) {
                populated = true;
                break;
              }
              await sleepMs(200);
            }
            if (!populated) await populate();
          }
        }
      }
      await setProjectPlayState(false);
      await ensureWorker();
      if (!cancelled) setStatus("ready");
    };
    run().catch(() => {
      if (!cancelled) setStatus("error");
    });
    return () => {
      cancelled = true;
    };
  }, [isProjectRealm]);

  return status;
}
