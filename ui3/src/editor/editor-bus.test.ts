import { afterEach, expect, it, vi } from "vitest";
import { createEditorBus, editorBusChannel, editorBusChannelFromViewportSrc } from "./editor-bus";

class Channel {
  static instances: Channel[] = [];
  onmessage: ((event: MessageEvent) => void) | null = null;
  closed = false;
  constructor(readonly name: string) { Channel.instances.push(this); }
  postMessage(data: unknown): void {
    for (const target of Channel.instances) {
      if (target !== this && !target.closed && target.name === this.name) target.onmessage?.({ data } as MessageEvent);
    }
  }
  close(): void { this.closed = true; }
}

afterEach(() => {
  Channel.instances = [];
  vi.unstubAllGlobals();
});

it("derives distinct namespaced transports only from valid viewport sessions", () => {
  const a = "/_play/?editorUi=1&editorSession=00000000-0000-4000-8000-000000000001";
  const b = "/_play/?editorUi=1&editorSession=00000000-0000-4000-8000-000000000002";
  expect(editorBusChannelFromViewportSrc(a)).toBe("dcl-editor-bus:00000000-0000-4000-8000-000000000001");
  expect(editorBusChannelFromViewportSrc(b)).toBe("dcl-editor-bus:00000000-0000-4000-8000-000000000002");
  expect(editorBusChannelFromViewportSrc("/_play/?editorUi=1")).toBeNull();
  expect(editorBusChannel("not-a-session")).toBeNull();
});

it("does not deliver one editor's messages to another editor namespace", () => {
  vi.stubGlobal("BroadcastChannel", Channel);
  const a = createEditorBus("/_play/?editorSession=00000000-0000-4000-8000-000000000001");
  const b = createEditorBus("/_play/?editorSession=00000000-0000-4000-8000-000000000002");
  const seenA = vi.fn();
  const seenB = vi.fn();
  a.onMessage(seenA);
  b.onMessage(seenB);
  const peer = new Channel("dcl-editor-bus:00000000-0000-4000-8000-000000000001");
  peer.postMessage({ to: "page", msg: { type: "tool", tool: "select" } });
  expect(seenA).toHaveBeenCalledOnce();
  expect(seenB).not.toHaveBeenCalled();
  a.close();
  b.close();
});

it("fails closed for a live viewport that has no session", () => {
  vi.stubGlobal("BroadcastChannel", Channel);
  expect(createEditorBus("/_play/?editorUi=1").ok).toBe(false);
  expect(createEditorBus().ok).toBe(false);
  expect(Channel.instances).toHaveLength(0);
});
