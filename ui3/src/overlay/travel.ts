import type { LifecycleSnapshot } from "../generated/bridge/LifecycleSnapshot";
import type { TravelOutcome } from "../generated/bridge/TravelOutcome";
import { OverlayPushSchema } from "../generated/bridge-schemas";
import { getBridge, subscribeLifecycle, type BridgeApi } from "./bridge";

export type TravelDestination = { realm?: string; parcel?: [number, number]; spawnPoint?: string };

export function travelInExplorer(
  destination: TravelDestination,
  options: { signal?: AbortSignal; bridge?: BridgeApi; requestId?: string; timeoutMs?: number } = {},
): Promise<TravelOutcome> {
  if (options.signal?.aborted) return Promise.reject(new DOMException("Stopped waiting for travel", "AbortError"));
  const bridge = options.bridge ?? getBridge();
  if (!bridge) return Promise.reject(new Error("Explorer is not attached"));
  const requestId = options.requestId ?? crypto.randomUUID();
  return new Promise((resolve, reject) => {
    let session: string | null = null;
    let lastRevision = -1;
    let finished = false;
    let unsubscribe: (() => void) | undefined;
    const finish = (outcome?: TravelOutcome, error?: Error) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      unsubscribe?.();
      options.signal?.removeEventListener("abort", abort);
      if (error) reject(error);
      else resolve(outcome!);
    };
    const abort = () => finish(undefined, new DOMException("Stopped waiting for travel", "AbortError"));
    const timer = setTimeout(() => finish(undefined, new Error("Explorer did not report arrival in time")), options.timeoutMs ?? 90000);
    const receive = (snapshot: LifecycleSnapshot) => {
      if (finished) return;
      if (session && snapshot.session !== session) {
        finish(undefined, new Error("Explorer restarted during travel"));
        return;
      }
      if (snapshot.revision <= lastRevision) return;
      lastRevision = snapshot.revision;
      if (session === null) {
        session = snapshot.session;
        try { (options.bridge ?? getBridge() ?? bridge).send("Travel", {
          requestId, expectedSession: session, realm: destination.realm ?? null,
          parcel: destination.parcel ?? null, spawnPoint: destination.spawnPoint ?? null,
        }); } catch (error) {
          finish(undefined, error instanceof Error ? error : new Error(String(error)));
          return;
        }
      }
      const rejected = snapshot.commandResults.find((result) => result.requestId === requestId && result.action === "travel" && !result.accepted);
      if (rejected) { finish(undefined, new Error(rejected.error ?? "Travel was rejected")); return; }
      const outcome = snapshot.outcomes.find((outcome) => outcome.operation.requestId === requestId && outcome.operation.session === session);
      if (!outcome) return;
      if (outcome.phase === "arrived" || outcome.phase === "degraded") finish(outcome);
      else {
        const error = new Error(outcome.reason ?? `Travel ${outcome.phase}`);
        if (outcome.phase === "cancelled") error.name = "TravelCancelledError";
        finish(undefined, error);
      }
    };
    try {
      unsubscribe = options.bridge ? options.bridge.onState((push) => {
        const result = OverlayPushSchema.safeParse(push);
        if (result.success && result.data.kind === "lifecycle") receive(result.data.snapshot);
      }) : subscribeLifecycle(receive);
    } catch (error) {
      finish(undefined, error instanceof Error ? error : new Error(String(error)));
      return;
    }
    if (finished) { unsubscribe(); return; }
    if (options.signal?.aborted) { abort(); return; }
    options.signal?.addEventListener("abort", abort, { once: true });
    try { bridge.send("GetLifecycleSnapshot", {}); }
    catch (error) { finish(undefined, error instanceof Error ? error : new Error(String(error))); }
  });
}
