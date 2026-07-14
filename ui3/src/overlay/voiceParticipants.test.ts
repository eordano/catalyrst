import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { VoiceParticipant } from "../generated/bridge/VoiceParticipant";
import { useVoiceParticipants } from "./voiceParticipants";

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

function participant(over: Partial<VoiceParticipant> = {}): VoiceParticipant {
  return {
    address: "0xabc0000000000000000000000000000000000001",
    name: "Ada",
    volume: 1,
    speaking: false,
    ...over,
  };
}

afterEach(() => {
  delete window.dclBridge;
  vi.useRealTimers();
});

describe("useVoiceParticipants", () => {
  it("attaches immediately and replaces the roster on each push", () => {
    const b = makeBridge();
    window.dclBridge = b.bridge;
    const { result } = renderHook(() => useVoiceParticipants());
    expect(result.current.participants).toEqual([]);
    act(() => b.push({ kind: "voiceParticipants", participants: [participant()] }));
    expect(result.current.participants).toHaveLength(1);
    act(() => b.push({ kind: "voiceParticipants", participants: [] }));
    expect(result.current.participants).toEqual([]);
  });

  it("ignores pushes of other kinds", () => {
    const b = makeBridge();
    window.dclBridge = b.bridge;
    const { result } = renderHook(() => useVoiceParticipants());
    act(() => b.push({ kind: "settings", settings: [] }));
    act(() => b.push(null));
    expect(result.current.participants).toEqual([]);
  });

  it("applies volume writes optimistically and sends SetVoiceParticipantVolume", () => {
    const b = makeBridge();
    window.dclBridge = b.bridge;
    const { result } = renderHook(() => useVoiceParticipants());
    act(() =>
      b.push({
        kind: "voiceParticipants",
        participants: [participant(), participant({ address: "0xdef", name: "Bob" })],
      }),
    );
    act(() => result.current.setVolume(participant().address, 0));
    expect(result.current.participants.find((p) => p.name === "Ada")?.volume).toBe(0);
    expect(result.current.participants.find((p) => p.name === "Bob")?.volume).toBe(1);
    expect(b.sent).toContainEqual({
      action: "SetVoiceParticipantVolume",
      payload: { address: participant().address, volume: 0 },
    });
    act(() =>
      b.push({ kind: "voiceParticipants", participants: [participant({ volume: 1 })] }),
    );
    expect(result.current.participants.find((p) => p.name === "Ada")?.volume).toBe(1);
  });

  it("polls every 250ms until the bridge appears", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useVoiceParticipants());
    const b = makeBridge();
    window.dclBridge = b.bridge;
    act(() => {
      vi.advanceTimersByTime(250);
    });
    act(() => b.push({ kind: "voiceParticipants", participants: [participant()] }));
    expect(result.current.participants).toHaveLength(1);
  });

  it("gives up after 10 seconds without a bridge", () => {
    vi.useFakeTimers();
    renderHook(() => useVoiceParticipants());
    act(() => {
      vi.advanceTimersByTime(10000);
    });
    const b = makeBridge();
    window.dclBridge = b.bridge;
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(b.listeners.size).toBe(0);
  });

  it("unsubscribes from the bridge on unmount", () => {
    const b = makeBridge();
    window.dclBridge = b.bridge;
    const { unmount } = renderHook(() => useVoiceParticipants());
    expect(b.listeners.size).toBe(1);
    unmount();
    expect(b.listeners.size).toBe(0);
  });
});
