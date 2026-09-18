import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";

import { ENGINE_RATE_STALE_MS, useFps } from "./useFps";

type Heartbeat = (fps?: number) => void;
const w = window as unknown as { __engineHeartbeat?: Heartbeat };

let clock = 0;
let queue: FrameRequestCallback[] = [];

function tick(ms: number) {
  clock += ms;
  const cbs = queue;
  queue = [];
  for (const cb of cbs) cb(clock);
}

function beat(fps?: number) {
  w.__engineHeartbeat?.(fps);
}

beforeEach(() => {
  clock = 1000;
  queue = [];
  vi.spyOn(performance, "now").mockImplementation(() => clock);
  vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => {
    queue.push(cb);
    return queue.length;
  });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {
    queue = [];
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  delete w.__engineHeartbeat;
});

describe("useFps", () => {
  test("counts per-frame heartbeats when the engine runs on the page thread", () => {
    const original = vi.fn();
    w.__engineHeartbeat = original;
    const { result } = renderHook(() => useFps(true));
    act(() => tick(16));
    for (let i = 0; i < 30; i += 1) beat();
    act(() => tick(500));
    expect(original).toHaveBeenCalledTimes(30);
    expect(result.current.engine).toBe(58);
    expect(result.current.page).toBe(4);
  });

  test("reads the rate a throttled worker heartbeat carries instead of counting posts", () => {
    const original = vi.fn();
    w.__engineHeartbeat = original;
    const { result } = renderHook(() => useFps(true));
    act(() => tick(16));
    beat(59.6);
    act(() => tick(500));
    expect(original).toHaveBeenCalledWith(59.6);
    expect(result.current.engine).toBe(60);

    beat(31.2);
    act(() => tick(500));
    expect(result.current.engine).toBe(31);
  });

  test("reports a silent engine as 0 once the last reported rate goes stale", () => {
    w.__engineHeartbeat = () => {};
    const { result } = renderHook(() => useFps(true));
    act(() => tick(16));
    beat(60);
    act(() => tick(500));
    expect(result.current.engine).toBe(60);
    act(() => {
      for (let t = 0; t <= ENGINE_RATE_STALE_MS; t += 500) tick(500);
    });
    expect(result.current.engine).toBe(0);
  });

  test("has no engine reading when the page exposes no heartbeat", () => {
    const { result } = renderHook(() => useFps(true));
    act(() => tick(16));
    act(() => tick(500));
    expect(result.current.engine).toBeNull();
  });

  test("restores the page heartbeat on unmount and stops counting", () => {
    const original = vi.fn();
    w.__engineHeartbeat = original;
    const { unmount } = renderHook(() => useFps(true));
    act(() => tick(16));
    expect(w.__engineHeartbeat).not.toBe(original);
    unmount();
    expect(w.__engineHeartbeat).toBe(original);
  });
});

describe("useFps nesting", () => {
  function frameScheduler() {
    const frames = new Map<number, FrameRequestCallback>();
    let nextId = 1;
    vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => {
      const id = nextId++;
      frames.set(id, cb);
      return id;
    });
    vi.spyOn(window, "cancelAnimationFrame").mockImplementation((id) => {
      frames.delete(id);
    });
    return (ms: number) => {
      clock += ms;
      const cbs = [...frames.values()];
      frames.clear();
      for (const cb of cbs) cb(clock);
    };
  }

  test("unwinding in mount order hands the heartbeat back one layer at a time", () => {
    const step = frameScheduler();
    const original = vi.fn();
    w.__engineHeartbeat = original;
    const a = renderHook(() => useFps(true));
    act(() => step(16));
    const wrapA = w.__engineHeartbeat;
    const b = renderHook(() => useFps(true));
    act(() => step(16));
    const wrapB = w.__engineHeartbeat;
    expect(wrapA).not.toBe(original);
    expect(wrapB).not.toBe(wrapA);

    b.unmount();
    expect(w.__engineHeartbeat).toBe(wrapA);
    a.unmount();
    expect(w.__engineHeartbeat).toBe(original);
  });

  test("unwinding out of order keeps the chain intact: beats still reach the page and the survivor keeps counting", () => {
    const step = frameScheduler();
    const original = vi.fn();
    w.__engineHeartbeat = original;
    const a = renderHook(() => useFps(true));
    act(() => step(16));
    const b = renderHook(() => useFps(true));
    act(() => step(16));
    const wrapB = w.__engineHeartbeat;

    a.unmount();
    expect(w.__engineHeartbeat).toBe(wrapB);
    for (let i = 0; i < 30; i += 1) beat();
    act(() => step(500));
    expect(original).toHaveBeenCalledTimes(30);
    expect(b.result.current.engine).toBe(58);
    expect(a.result.current.engine).toBeNull();

    b.unmount();
    beat();
    expect(original).toHaveBeenCalledTimes(31);
  });
});
