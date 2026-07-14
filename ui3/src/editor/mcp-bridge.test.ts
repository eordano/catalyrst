// Pins the pairing trust boundary: ?mcp= may pair a LOOPBACK relay silently (the
// documented rig workflow), but a remote ws(s):// relay -- which gets
// screenshots, play control and asset writes -- must not open a socket until the
// caller's consent surface approves it, and is refused outright when no consent
// surface exists. A crafted link must produce zero sockets.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { autoConnect } from "./mcp-bridge";

class FakeWebSocket {
  static urls: string[] = [];
  static OPEN = 1;
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((ev: unknown) => void) | null = null;
  onclose: ((ev: unknown) => void) | null = null;
  onerror: (() => void) | null = null;
  constructor(url: string) {
    FakeWebSocket.urls.push(url);
  }
  send(): void {}
  close(): void {}
}

function setPageUrl(search: string, hash: string): void {
  window.history.replaceState(null, "", `/creator-hub/scene-editor${search}`);
  window.location.hash = hash;
}

const flush = () => new Promise<void>((r) => setTimeout(r, 0));

let dispose: (() => void) | null = null;

beforeEach(() => {
  FakeWebSocket.urls = [];
  vi.stubGlobal("WebSocket", FakeWebSocket);
  if (typeof globalThis.BroadcastChannel === "undefined") {
    vi.stubGlobal(
      "BroadcastChannel",
      class {
        onmessage: unknown = null;
        postMessage(): void {}
        close(): void {}
      },
    );
  }
  window.localStorage.clear();
});

afterEach(() => {
  dispose?.();
  dispose = null;
  vi.unstubAllGlobals();
  setPageUrl("", "");
});

describe("mcp pairing gate", () => {
  it("pairs a loopback port silently, never asking for consent", () => {
    setPageUrl("?mcp=5196", "#mcptoken=tok");
    const confirmRemote = vi.fn();
    dispose = autoConnect({ confirmRemote });
    expect(FakeWebSocket.urls).toEqual(["ws://127.0.0.1:5196/bridge"]);
    expect(confirmRemote).not.toHaveBeenCalled();
  });

  it("treats the whole 127.0.0.0/8 block and localhost as loopback", () => {
    setPageUrl(`?mcp=${encodeURIComponent("ws://127.0.0.5:5196/bridge")}`, "#mcptoken=tok");
    dispose = autoConnect({ confirmRemote: vi.fn() });
    expect(FakeWebSocket.urls).toEqual(["ws://127.0.0.5:5196/bridge"]);
  });

  it("opens no socket for a remote relay until consent resolves", async () => {
    setPageUrl(`?mcp=${encodeURIComponent("wss://evil.example/bridge")}`, "#mcptoken=tok");
    let resolveConsent: ((ok: boolean) => void) | undefined;
    const confirmRemote = vi.fn(
      (host: string) =>
        new Promise<boolean>((resolve) => {
          void host;
          resolveConsent = resolve;
        }),
    );
    dispose = autoConnect({ confirmRemote });
    expect(confirmRemote).toHaveBeenCalledWith("evil.example");
    expect(FakeWebSocket.urls).toEqual([]);
    resolveConsent?.(false);
    await flush();
    expect(FakeWebSocket.urls).toEqual([]);
  });

  it("approving connects to exactly the named relay", async () => {
    setPageUrl(`?mcp=${encodeURIComponent("wss://relay.tail.example/bridge")}`, "#mcptoken=tok");
    dispose = autoConnect({ confirmRemote: () => Promise.resolve(true) });
    await flush();
    expect(FakeWebSocket.urls).toEqual(["wss://relay.tail.example/bridge"]);
  });

  it("refuses a remote relay outright when no consent surface exists", async () => {
    setPageUrl(`?mcp=${encodeURIComponent("wss://evil.example/bridge")}`, "#mcptoken=tok");
    dispose = autoConnect();
    await flush();
    expect(FakeWebSocket.urls).toEqual([]);
  });

  it("a stored remote config is gated too", () => {
    window.localStorage.setItem(
      "dcl-mcp-relay",
      JSON.stringify({ url: "wss://far.example/bridge", token: "tok" }),
    );
    setPageUrl("", "");
    const confirmRemote = vi.fn(() => new Promise<boolean>(() => {}));
    dispose = autoConnect({ confirmRemote });
    expect(FakeWebSocket.urls).toEqual([]);
    expect(confirmRemote).toHaveBeenCalledWith("far.example");
  });

  it("a stored loopback config still pairs silently", () => {
    window.localStorage.setItem("dcl-mcp-relay", JSON.stringify({ url: 5196, token: "tok" }));
    setPageUrl("", "");
    dispose = autoConnect({ confirmRemote: vi.fn() });
    expect(FakeWebSocket.urls).toEqual(["ws://127.0.0.1:5196/bridge"]);
  });

  it("disposing before consent resolves cancels the pairing", async () => {
    setPageUrl(`?mcp=${encodeURIComponent("wss://slow.example/bridge")}`, "#mcptoken=tok");
    let resolveConsent: ((ok: boolean) => void) | undefined;
    dispose = autoConnect({
      confirmRemote: () =>
        new Promise<boolean>((resolve) => {
          resolveConsent = resolve;
        }),
    });
    dispose?.();
    dispose = null;
    resolveConsent?.(true);
    await flush();
    expect(FakeWebSocket.urls).toEqual([]);
  });
});
