import type { LifecycleSnapshot } from "../generated/bridge/LifecycleSnapshot";
import { OverlayPushSchema } from "../generated/bridge-schemas";
import { getBridge, subscribeLifecycle, type BridgeApi, type BridgePayloads } from "./bridge";

export type LifecycleCommand = {
  [K in "CancelTravel" | "RetryConnection"]: { action: K; payload: BridgePayloads[K] };
}["CancelTravel" | "RetryConnection"];

export function lifecycleCommand(
  command: LifecycleCommand,
  options: { bridge?: BridgeApi; signal?: AbortSignal; timeoutMs?: number } = {},
): Promise<void> {
  if (options.signal?.aborted) return Promise.reject(new DOMException("Stopped waiting for explorer", "AbortError"));
  const bridge = options.bridge ?? getBridge();
  if (!bridge) return Promise.reject(new Error("Explorer is not attached"));
  const kind = command.action === "CancelTravel" ? "cancelTravel" : "retryConnection";
  return new Promise((resolve, reject) => {
    let finished = false;
    let revision = -1;
    let unsubscribe: (() => void) | undefined;
    const finish = (error?: Error) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      unsubscribe?.();
      options.signal?.removeEventListener("abort", abort);
      if (error) reject(error);
      else resolve();
    };
    const abort = () => finish(new DOMException("Stopped waiting for explorer", "AbortError"));
    const timer = setTimeout(() => finish(new Error("Explorer did not acknowledge the request. Try again.")), options.timeoutMs ?? 15000);
    const receive = (snapshot: LifecycleSnapshot) => {
      if (finished) return;
      if (snapshot.session !== command.payload.expectedSession) {
        finish(new Error("Explorer restarted. Try again in the current session."));
        return;
      }
      if (snapshot.revision <= revision) return;
      revision = snapshot.revision;
      const response = snapshot.commandResults.find((result) =>
        result.requestId === command.payload.requestId && result.action === kind);
      if (response) finish(response.accepted ? undefined : new Error(response.error ?? "Explorer rejected the request."));
    };
    try {
      unsubscribe = options.bridge ? options.bridge.onState((push) => {
        const parsed = OverlayPushSchema.safeParse(push);
        if (parsed.success && parsed.data.kind === "lifecycle") receive(parsed.data.snapshot);
      }) : subscribeLifecycle(receive);
      if (finished) { unsubscribe(); return; }
      if (options.signal?.aborted) { abort(); return; }
      options.signal?.addEventListener("abort", abort, { once: true });
      if (command.action === "CancelTravel") bridge.send(command.action, command.payload);
      else bridge.send(command.action, command.payload);
      bridge.send("GetLifecycleSnapshot", {});
    } catch (error) {
      finish(error instanceof Error ? error : new Error(String(error)));
    }
  });
}
