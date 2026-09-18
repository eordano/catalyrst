import { act, renderHook } from "@testing-library/react";
import type { RefObject } from "react";
import { afterEach, expect, test, vi } from "vitest";
import type { EditorBus } from "../editor-bus";
import type { LiveSceneInfo } from "../../generated/editor-bus";
import { useSceneMeters } from "./useSceneMeters";

const scene = { parcels: ["0,0"] } as unknown as LiveSceneInfo;
const flush = async () => { await Promise.resolve(); await Promise.resolve(); };
const bus = (rpc: (method: string) => Promise<unknown>) => ({ rpc } as unknown as EditorBus);
const options = (busRef: RefObject<EditorBus | null>) => ({ busRef, busLive: true, scene });

afterEach(() => vi.useRealTimers());

test("waits for a sceneStats reply before scheduling the next meter read", async () => {
  vi.useFakeTimers();
  let resolve!: (value: unknown) => void;
  const rpc = vi.fn(() => new Promise<unknown>((done) => { resolve = done; }));
  const busRef = { current: bus(rpc) } as RefObject<EditorBus | null>;
  const rendered = renderHook(() => useSceneMeters(options(busRef)));

  expect(rpc).toHaveBeenCalledTimes(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });
  expect(rpc).toHaveBeenCalledTimes(1);

  await act(async () => { resolve("entities: 7"); await flush(); });
  expect(rendered.result.current).toEqual([{ label: "entities", value: 7, limit: 200 }]);
  await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
  expect(rpc).toHaveBeenCalledTimes(2);
  rendered.unmount();
});

test("ignores a replaced bus result and does not reschedule after cleanup", async () => {
  vi.useFakeTimers();
  let resolve!: (value: unknown) => void;
  const firstRpc = vi.fn(() => new Promise<unknown>((done) => { resolve = done; }));
  const secondRpc = vi.fn(async () => "entities: 3");
  const busRef = { current: bus(firstRpc) } as RefObject<EditorBus | null>;
  const rendered = renderHook(() => useSceneMeters(options(busRef)));

  busRef.current = bus(secondRpc);
  await act(async () => { resolve("entities: 99"); await flush(); });
  expect(rendered.result.current).toEqual([]);
  await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
  expect(secondRpc).toHaveBeenCalledTimes(1);
  expect(rendered.result.current).toEqual([{ label: "entities", value: 3, limit: 200 }]);

  rendered.unmount();
  await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
  expect(secondRpc).toHaveBeenCalledTimes(1);
});
