import { catalystBase } from "./client";

const WORKERS = 8;

export function warmSceneAtParcel(x: number, y: number): void {
  if (typeof navigator === "undefined" || !navigator.serviceWorker?.controller) return;
  const base = `${catalystBase()}/content`;
  void (async () => {
    try {
      const res = await fetch(`${base}/entities/active`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ pointers: [`${x},${y}`] }),
      });
      if (!res.ok) return;
      const entities = (await res.json()) as Array<{
        id?: string;
        content?: Array<{ hash?: string }>;
      }>;
      const entity = entities?.[0];
      if (!entity) return;
      const queue = [
        ...new Set(
          (entity.content ?? [])
            .map((c) => c.hash)
            .filter((h): h is string => typeof h === "string"),
        ),
      ];
      if (entity.id) queue.push(entity.id);
      // X-IPFS routes these through the service worker's cache-first strategy,
      // so the engine's own fetches for the destination scene hit a warm cache.
      await Promise.all(
        Array.from({ length: WORKERS }, async () => {
          for (let h = queue.pop(); h; h = queue.pop()) {
            try {
              const r = await fetch(`${base}/contents/${h}`, {
                headers: { "X-IPFS": "true" },
              });
              await r.arrayBuffer();
            } catch {
            }
          }
        }),
      );
    } catch {
    }
  })();
}
