import assert from "node:assert";
import { afterEach, describe, it } from "node:test";

import { portalSource } from "./portal";

const KEYS = ["SQD_PORTAL_API_KEY", "SQD_PORTAL_URL"] as const;
const saved: Record<string, string | undefined> = {};
for (const k of KEYS) saved[k] = process.env[k];

afterEach(() => {
  for (const k of KEYS) {
    if (saved[k] === undefined) delete process.env[k];
    else process.env[k] = saved[k];
  }
});

describe("portalSource", () => {
  it("falls back to the un-authed public endpoint when no key is wired", () => {
    delete process.env.SQD_PORTAL_API_KEY;
    delete process.env.SQD_PORTAL_URL;
    assert.strictEqual(
      portalSource("polygon-mainnet"),
      "https://portal.sqd.dev/datasets/polygon-mainnet"
    );
  });

  it("uses the authenticated shared endpoint when a key is present", () => {
    process.env.SQD_PORTAL_API_KEY = "k-123";
    delete process.env.SQD_PORTAL_URL;
    assert.deepStrictEqual(portalSource("ethereum-mainnet"), {
      url: "https://shared.portal.sqd.dev/datasets/ethereum-mainnet",
      http: { headers: { "x-api-key": "k-123" } },
    });
  });

  it("honors SQD_PORTAL_URL as the host override on the authed path", () => {
    process.env.SQD_PORTAL_API_KEY = "k-123";
    process.env.SQD_PORTAL_URL = "https://moved.portal.example";
    assert.deepStrictEqual(portalSource("polygon-mainnet"), {
      url: "https://moved.portal.example/datasets/polygon-mainnet",
      http: { headers: { "x-api-key": "k-123" } },
    });
  });

  it("ignores SQD_PORTAL_URL when no key is set (stays public)", () => {
    delete process.env.SQD_PORTAL_API_KEY;
    process.env.SQD_PORTAL_URL = "https://moved.portal.example";
    assert.strictEqual(
      portalSource("ethereum-mainnet"),
      "https://portal.sqd.dev/datasets/ethereum-mainnet"
    );
  });
});
