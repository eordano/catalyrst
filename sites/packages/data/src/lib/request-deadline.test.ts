import { afterEach, expect, it, vi } from "vitest";
import { withDeadline } from "./request-deadline";

afterEach(() => vi.useRealTimers());

it("cleans up timers and parent listeners when a dependency throws synchronously", async () => {
  vi.useFakeTimers();
  const parent = new AbortController();
  const removed = vi.spyOn(parent.signal, "removeEventListener");
  await expect(withDeadline(() => { throw new Error("failed"); }, 1000, parent.signal)).rejects.toThrow("failed");
  expect(removed).toHaveBeenCalledWith("abort", expect.any(Function));
  expect(vi.getTimerCount()).toBe(0);
});

it("preserves the caller's cancellation reason and aborts the underlying operation", async () => {
  vi.useFakeTimers();
  const parent = new AbortController();
  let requestSignal: AbortSignal | undefined;
  const pending = withDeadline((signal) => {
    requestSignal = signal;
    return new Promise(() => {});
  }, 1000, parent.signal);
  const cancelled = expect(pending).rejects.toBe("navigation changed");
  await vi.advanceTimersByTimeAsync(0);
  parent.abort("navigation changed");
  await cancelled;
  expect(requestSignal?.reason).toBe("navigation changed");
  expect(vi.getTimerCount()).toBe(0);
});
