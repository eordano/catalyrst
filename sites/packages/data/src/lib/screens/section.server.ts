import type { ScreenSection } from "@ui/data/screens/section";
import { withDeadline } from "../request-deadline";

export function screenSections(signal?: AbortSignal) {
  const started = performance.now();
  const timings: string[] = [];
  return {
    async load<T>(name: string, load: (signal: AbortSignal) => Promise<T>, timeoutMs = 3_000): Promise<ScreenSection<T>> {
      const at = performance.now();
      let status = "unavailable";
      try {
        const data = await withDeadline(load, timeoutMs, signal);
        status = "ready";
        return { status: "ready", data, updatedAt: Date.now() };
      } catch {
        signal?.throwIfAborted();
        return { status: "unavailable", data: null, updatedAt: null };
      } finally {
        timings.push(`${name};dur=${(performance.now() - at).toFixed(1)};desc="${status}"`);
      }
    },
    timing() {
      return [...timings, `screen_total;dur=${(performance.now() - started).toFixed(1)}`].join(", ");
    },
  };
}
