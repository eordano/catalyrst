import { describe, expect, test } from "vitest";

import {
  AuthIdentitySchema,
  DevSignerKeySchema,
  PendingCheckoutStoreSchema,
  PendingTopupStoreSchema,
  PersistedFavoritesSchema,
  SimCollectionItemsStoreSchema,
  ThirdwebSessionSchema,
} from "./persisted-schemas";

// Every case below is drift a persisted blob can actually carry -- written by
// an older build, read by this one -- and every one of them got past the guard
// that already shipped at that read site. Each case asserts BOTH halves: the
// schema rejects it and the old guard did not. A case the old guard already
// caught would prove nothing, which is why the guards are spelled out here
// rather than described.

type Case = [name: string, value: unknown, schemaAccepts: boolean];

function table(
  schema: { safeParse(v: unknown): { success: boolean } },
  guard: (v: unknown) => boolean,
  cases: Case[],
): void {
  for (const [name, value, schemaAccepts] of cases) {
    test(name, () => {
      expect(schema.safeParse(value).success).toBe(schemaAccepts);
      expect(guard(value)).toBe(true);
    });
  }
}

describe("auth identity", () => {
  // session.ts: signer truthy, a private key present, authChain an array.
  const guard = (v: unknown) => {
    const p = v as {
      signer?: unknown;
      ephemeral?: { privateKey?: unknown };
      authChain?: unknown;
    };
    return Boolean(p?.signer && p?.ephemeral?.privateKey && Array.isArray(p.authChain));
  };
  // A real ephemeral key is secp256k1 -- 32 bytes, 64 hex chars -- which is the
  // shape DevSignerKeySchema below already pins. The old "0xkey" placeholder was
  // not even valid hex, so "what this build writes" asserted against something
  // this build never writes, and it was the only thing holding HexKeySchema to a
  // bare startsWith("0x").
  const EPHEMERAL_KEY = `0x${"a1b2c3d4".repeat(8)}`;
  const identity = (over: Record<string, unknown> = {}) => ({
    signer: "0xabc",
    ephemeral: { address: "0xeee", privateKey: EPHEMERAL_KEY },
    expiration: "2099-01-01T00:00:00.000Z",
    authChain: [{ type: "SIGNER", payload: "0xabc", signature: "" }],
    ...over,
  });

  table(AuthIdentitySchema, guard, [
    ["what this build writes", identity(), true],
    [
      "a private key stored without its 0x prefix",
      identity({ ephemeral: { address: "0xeee", privateKey: "deadbeef" } }),
      false,
    ],
    [
      "an identity written before it carried an expiration",
      identity({ expiration: undefined }),
      false,
    ],
    [
      "an auth link missing its signature",
      identity({ authChain: [{ type: "SIGNER", payload: "0xabc" }] }),
      false,
    ],
    [
      "an auth link type this build does not know",
      identity({ authChain: [{ type: "EIP1271", payload: "0xabc", signature: "0x1" }] }),
      false,
    ],
  ]);
});

describe("thirdweb session", () => {
  // thirdweb/session.ts: both fields merely truthy.
  const guard = (v: unknown) => {
    const p = v as { token?: unknown; address?: unknown };
    return Boolean(p?.token && p?.address);
  };

  table(ThirdwebSessionSchema, guard, [
    ["what this build writes", { token: "eyJ...", address: "0xabc" }, true],
    ["a token stored as its expiry epoch", { token: 1893456000, address: "0xabc" }, false],
    [
      "an address that became the whole account object",
      { token: "eyJ...", address: { value: "0xabc" } },
      false,
    ],
  ]);
});

describe("pending purchases", () => {
  // pending-store.ts: "is it an object", plus a per-entry business rule that
  // looks at exactly one field. Neither of them looks at `ts`, which is what
  // decides whether an entry has expired.
  const guard = (v: unknown) => Boolean(v && typeof v === "object");

  table(PendingTopupStoreSchema, guard, [
    ["what this build writes", { "0xabc": { txHash: "0x1", ts: 1_700_000_000_000 } }, true],
    [
      "a timestamp stored as an ISO string, which never looks expired",
      { "0xabc": { txHash: "0x1", ts: "2026-01-01T00:00:00.000Z" } },
      false,
    ],
    ["an entry keyed straight to its hash", { "0xabc": "0x1" }, false],
  ]);

  table(PendingCheckoutStoreSchema, guard, [
    ["what this build writes", { "0xabc": { checkoutId: 42, ts: 1_700_000_000_000 } }, true],
    [
      "a checkout written before entries carried a timestamp",
      { "0xabc": { checkoutId: 42 } },
      false,
    ],
    [
      "a checkout id that became the server's opaque string",
      { "0xabc": { checkoutId: "ck_42", ts: 1_700_000_000_000 } },
      false,
    ],
  ]);
});

describe("sim collection items", () => {
  // sim-collection-items.ts: "is it an object", and `prune` only checks that
  // `files` is an array and `ts` a number -- never what a file looks like.
  const guard = (v: unknown) => Boolean(v && typeof v === "object");
  const entry = (files: unknown[]) => ({ c1: { ts: 1_700_000_000_000, files } });

  table(SimCollectionItemsStoreSchema, guard, [
    ["what this build writes", entry([{ name: "hat.glb", size: 12, fileType: "model/gltf" }]), true],
    ["a file size stored as a formatted string", entry([{ name: "hat.glb", size: "12 KB", fileType: "model/gltf" }]), false],
    ["a draft reduced to its file names", entry(["hat.glb"]), false],
  ]);
});

describe("dev signer key", () => {
  // dev-identity.ts: startsWith("0x") and nothing else.
  const guard = (v: unknown) => typeof v === "string" && v.startsWith("0x");
  const key = `0x${"a".repeat(64)}`;

  table(DevSignerKeySchema, guard, [
    ["a real burner key", key, true],
    ["a truncated key, which viem would throw on", "0xdeadbeef", false],
    ["an address stored where the key belongs", `0x${"b".repeat(40)}`, false],
  ]);
});

describe("shop favorites", () => {
  // favorites.ts: Array.isArray, so a list of anything was a list of cards.
  const guard = (v: unknown) => Array.isArray(v);
  const card = (over: Record<string, unknown> = {}) => ({
    id: "urn:a:1",
    name: "Hat",
    meta: "Collectible",
    price: 500,
    unit: "mana",
    rarity: "rare",
    network: "polygon",
    image: "https://example/hat.png",
    ...over,
  });

  table(PersistedFavoritesSchema, guard, [
    ["what this build writes", [card()], true],
    ["a card written before `unit` existed, which the read migrates", [card({ unit: undefined })], true],
    [
      "a price that became a formatted amount object",
      [card({ price: { amount: "500", currency: "MANA" } })],
      false,
    ],
    ["a network this build has no branch for", [card({ network: "base" })], false],
    ["an older build's list of urns", ["urn:a:1"], false],
  ]);
});
