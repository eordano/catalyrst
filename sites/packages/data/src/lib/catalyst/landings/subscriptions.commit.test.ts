import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../auth/signer", () => ({ signedFetch: vi.fn() }));
vi.mock("../client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../client")>();
  return { ...actual, signedGetJSON: vi.fn() };
});

import { signedFetch } from "../../auth/signer";
import { signedGetJSON } from "../client";
import { buildSubscriptionCommit } from "./subscriptions";
import type { AuthIdentity } from "../../auth/types";

const mSignedFetch = vi.mocked(signedFetch);
const mSignedGet = vi.mocked(signedGetJSON);

const IDENTITY: AuthIdentity = {
  signer: "0x4e9c4a2502fdf71e93ed8ed6ca9ddbd891d6f295",
  ephemeral: { address: "0xeph", privateKey: "0xdeadbeef" },
  expiration: "2999-01-01T00:00:00.000Z",
  authChain: [],
};

function currentSubscription() {
  return {
    address: IDENTITY.signer,
    email: "user@example.com",
    details: {
      ignore_all_email: true,
      ignore_all_in_app: false,
      message_type: {
        events_started: { email: false, in_app: true },
        events_starts_soon: { email: false, in_app: false },
      },
    },
  };
}

function putBody(): Record<string, unknown> {
  const init = mSignedFetch.mock.calls[0][2] as { body: string };
  return JSON.parse(init.body);
}

beforeEach(() => {
  vi.clearAllMocks();
  mSignedFetch.mockResolvedValue(new Response(null, { status: 200 }));
});

describe("buildSubscriptionCommit \u{2014} real notifications subscription write", () => {
  it("subscribe reads current details, enables email for the selected types, and PUTs", async () => {
    mSignedGet.mockResolvedValue(currentSubscription());
    const commit = buildSubscriptionCommit(IDENTITY);

    const out = await commit({
      kind: "subscribe",
      enabledTypes: ["events_started"],
    });

    expect(out.kind).toBe("subscribe");
    expect(Object.keys(out).sort()).toEqual(["at", "kind"]);
    expect(mSignedFetch).toHaveBeenCalledTimes(1);
    const [, url, init] = mSignedFetch.mock.calls[0];
    expect(String(url)).toMatch(/\/subscription$/);
    expect((init as { method: string }).method).toBe("PUT");

    const body = putBody();
    expect(body.ignore_all_email).toBe(false);
    expect(body.ignore_all_in_app).toBe(false);
    const mt = body.message_type as Record<string, { email: boolean; in_app: boolean }>;
    expect(mt.events_started).toEqual({ email: true, in_app: true });
    expect(mt.events_starts_soon).toEqual({ email: false, in_app: false });
  });

  it("unsubscribe sets ignore_all_email without flipping per-type flags", async () => {
    mSignedGet.mockResolvedValue(currentSubscription());
    const commit = buildSubscriptionCommit(IDENTITY);

    await commit({ kind: "unsubscribe", enabledTypes: ["events_started"] });

    const body = putBody();
    expect(body.ignore_all_email).toBe(true);
    const mt = body.message_type as Record<string, { email: boolean }>;
    expect(mt.events_started.email).toBe(false);
  });

  it("fails closed without an identity \u{2014} no read, no write", async () => {
    const commit = buildSubscriptionCommit(null);
    await expect(
      commit({ kind: "subscribe", enabledTypes: [] }),
    ).rejects.toThrow(/sign in/i);
    expect(mSignedGet).not.toHaveBeenCalled();
    expect(mSignedFetch).not.toHaveBeenCalled();
  });

  it("propagates a non-ok PUT as an error (no silent success)", async () => {
    mSignedGet.mockResolvedValue(currentSubscription());
    mSignedFetch.mockResolvedValue(new Response(null, { status: 401 }));
    const commit = buildSubscriptionCommit(IDENTITY);

    await expect(
      commit({ kind: "subscribe", enabledTypes: ["events_started"] }),
    ).rejects.toThrow(/401/);
  });
});
