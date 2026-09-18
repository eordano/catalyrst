import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { SettingEntry } from "../generated/bridge/SettingEntry";
import { useEngineSettings } from "./engineSettings";

type Sent = { action: string; payload: unknown };

function makeBridge() {
  const listeners = new Set<(push: unknown) => void>();
  const sent: Sent[] = [];
  const bridge = {
    send: (action: string, payload?: unknown) => {
      sent.push({ action, payload });
    },
    onState: (cb: (push: unknown) => void) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
  const push = (p: unknown) => {
    for (const cb of [...listeners]) cb(p);
  };
  return { bridge, sent, push, listeners };
}

const variant = (name: string, description = "") => ({ name, description });

function entry(over: Partial<SettingEntry> = {}): SettingEntry {
  return {
    name: "Bloom",
    category: "Graphics",
    description: "",
    minValue: 0,
    maxValue: 2,
    namedVariants: [variant("Off"), variant("Low"), variant("High")],
    stepSize: 1,
    value: 1,
    default: 1,
    ...over,
  };
}

const outlineEntry = (over: Partial<SettingEntry> = {}): SettingEntry =>
  entry({
    name: "Avatar Outline",
    namedVariants: [variant("Always"), variant("Focus"), variant("Off")],
    value: 1,
    ...over,
  });

afterEach(() => {
  delete window.dclBridge;
  vi.useRealTimers();
});

describe("useEngineSettings", () => {
  it("attaches immediately, requests a snapshot, normalizes variants and descriptions (including legacy bare strings), and unsubscribes on unmount", () => {
    const b = makeBridge();
    window.dclBridge = b.bridge;
    const { result, unmount } = renderHook(() => useEngineSettings());
    expect(b.sent).toContainEqual({ action: "GetSettings", payload: {} });
    expect(result.current.connected).toBe(true);

    act(() =>
      b.push({
        kind: "settings",
        settings: [
          entry({
            description: "Glow around bright light sources.",
            namedVariants: [variant("Off", "No glow."), variant("High", "Strong glow.")],
          }),
        ],
      }),
    );
    expect(result.current.values.Bloom).toBe(1);
    expect(result.current.info?.Bloom?.description).toBe("Glow around bright light sources.");
    expect(result.current.info?.Bloom?.namedVariants).toEqual([
      { label: "Off", description: "No glow." },
      { label: "High", description: "Strong glow." },
    ]);

    const legacy = { ...entry(), namedVariants: ["Off", "Low", "High"] } as unknown as SettingEntry;
    act(() => b.push({ kind: "settings", settings: [legacy] }));
    expect(result.current.info?.Bloom?.namedVariants).toEqual([
      { label: "Off", description: null },
      { label: "Low", description: null },
      { label: "High", description: null },
    ]);
    expect(result.current.info?.Bloom?.description).toBeNull();

    expect(b.listeners.size).toBe(1);
    unmount();
    expect(b.listeners.size).toBe(0);
  });

  it("merges values across snapshot pushes and ignores pushes of other kinds", () => {
    const b = makeBridge();
    window.dclBridge = b.bridge;
    const { result } = renderHook(() => useEngineSettings());
    act(() => b.push({ kind: "voiceParticipants", participants: [] }));
    act(() => b.push(null));
    expect(result.current.info).toBeNull();
    expect(result.current.values).toEqual({});

    act(() => b.push({ kind: "settings", settings: [entry({ value: 2 })] }));
    act(() => b.push({ kind: "settings", settings: [outlineEntry({ value: 0 })] }));
    expect(result.current.values.Bloom).toBe(2);
    expect(result.current.values["Avatar Outline"]).toBe(0);
  });

  it("applies a write optimistically, sends SetSetting, and lets the echo reconcile", () => {
    const b = makeBridge();
    window.dclBridge = b.bridge;
    const { result } = renderHook(() => useEngineSettings());
    act(() => result.current.setValue("Avatar Outline", 2));
    expect(result.current.values["Avatar Outline"]).toBe(2);
    expect(b.sent).toContainEqual({
      action: "SetSetting",
      payload: { name: "Avatar Outline", value: 2 },
    });
    act(() => b.push({ kind: "settings", settings: [outlineEntry({ value: 1 })] }));
    expect(result.current.values["Avatar Outline"]).toBe(1);
  });

  it("polls every 250ms until the bridge appears and gives up after 10 seconds", () => {
    vi.useFakeTimers();
    const late = renderHook(() => useEngineSettings());
    expect(late.result.current.connected).toBeNull();
    const b = makeBridge();
    window.dclBridge = b.bridge;
    expect(b.sent).toHaveLength(0);
    act(() => {
      vi.advanceTimersByTime(250);
    });
    expect(b.sent).toContainEqual({ action: "GetSettings", payload: {} });
    expect(late.result.current.connected).toBe(true);
    late.unmount();
    delete window.dclBridge;

    const never = renderHook(() => useEngineSettings());
    act(() => {
      vi.advanceTimersByTime(10000);
    });
    expect(never.result.current.connected).toBe(false);
    const c = makeBridge();
    window.dclBridge = c.bridge;
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(c.sent).toHaveLength(0);
    expect(c.listeners.size).toBe(0);
    expect(never.result.current.connected).toBe(false);
  });
});
