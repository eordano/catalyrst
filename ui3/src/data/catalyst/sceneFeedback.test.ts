import { afterEach, describe, expect, it, vi } from "vitest";

import { feedbackRoute, sendOwnerMessage } from "./sceneFeedback";
import { CatalystError } from "./client";

const OWNER = "0xD6EFF8F07CAF3443A1178407D3DE4129149D6EF6";
const owner = { address: OWNER.toLowerCase() };
const other = { address: "0x0000000000000000000000000000000000000009" };

describe("feedbackRoute", () => {
  it("messages accepted friends, refuses while a request is pending either way, and asks strangers to connect", () => {
    expect(feedbackRoute(OWNER, [owner], [], [])).toBe("message");
    expect(feedbackRoute(OWNER, [owner], [owner], [owner])).toBe("message");
    expect(feedbackRoute(OWNER, [], [owner], [])).toBe("pending");
    expect(feedbackRoute(OWNER, [], [], [owner])).toBe("incoming");
    expect(feedbackRoute(OWNER, [other], [other], [other])).toBe("request");
    expect(feedbackRoute(OWNER, [], [], [])).toBe("request");
  });
});

type FakeBridge = {
  send: ReturnType<typeof vi.fn>;
  onState: (cb: (push: unknown) => void) => () => void;
};

function installFakeBridge(reply: (payload: { id: string; url: string; body?: string }) => { status: number; body: string }) {
  const listeners = new Set<(push: unknown) => void>();
  const bridge: FakeBridge = {
    send: vi.fn((action: string, payload: { id: string; url: string; body?: string }) => {
      if (action !== "SignedFetch") return;
      const { status, body } = reply(payload);
      queueMicrotask(() => {
        for (const cb of listeners) cb({ kind: "signedFetchResult", id: payload.id, status, body });
      });
    }),
    onState: (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
  (window as unknown as { dclBridge: FakeBridge }).dclBridge = bridge;
  return bridge;
}

describe("sendOwnerMessage", () => {
  afterEach(() => {
    delete (window as unknown as { dclBridge?: FakeBridge }).dclBridge;
  });

  it("posts the note to the social service DM endpoint through a signed fetch", async () => {
    const bridge = installFakeBridge(() => ({
      status: 200,
      body: JSON.stringify({
        message: { id: "7", from: "0xme", to: owner.address, body: "hi", sentAt: "2026-09-17T00:00:00Z" },
      }),
    }));
    const out = await sendOwnerMessage(OWNER, "hi");
    expect(out?.message.id).toBe("7");
    const [action, payload] = bridge.send.mock.calls[0] as [string, { url: string; method: string; body: string }];
    expect(action).toBe("SignedFetch");
    expect(payload.method).toBe("POST");
    expect(payload.url).toBe(`${window.location.origin}/v1/friends/${owner.address}/messages`);
    expect(JSON.parse(payload.body)).toEqual({ body: "hi" });
  });

  it("surfaces the social service refusal, and fails when the engine bridge is not there to sign", async () => {
    installFakeBridge(() => ({ status: 403, body: JSON.stringify({ message: "not accepted friends" }) }));
    await expect(sendOwnerMessage(OWNER, "hi")).rejects.toMatchObject({
      name: "CatalystError",
      status: 403,
      message: "not accepted friends",
    } satisfies Partial<CatalystError>);
    delete (window as unknown as { dclBridge?: FakeBridge }).dclBridge;
    await expect(sendOwnerMessage(OWNER, "hi")).rejects.toBeInstanceOf(CatalystError);
  });
});
