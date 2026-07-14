import { describe, expect, test } from "vitest";
import {
  RecentPlacesSchema,
  StoredAuthIdentitySchema,
} from "../data/persisted-schemas";

// Every case below is drift a persisted blob can actually carry -- a value
// written by an older build and read by this one -- and every one of them got
// past the guard that shipped before the schema landed. Each case asserts BOTH
// halves: the schema rejects it and the old guard did not. A case the old guard
// already caught would prove nothing.

function without<T extends object>(o: T, key: keyof T): Omit<T, keyof T> {
  const copy = { ...o };
  delete copy[key];
  return copy;
}

/** `isStoredIdentity`, as it stood in engineLogin.ts and (duplicated) in
 *  signedFetchLocal.ts. It never looked inside `authChain` at all. */
const oldIdentityGuard = (v: unknown) => {
  const s = v as {
    ephemeralIdentity?: { privateKey?: unknown };
    expiration?: unknown;
    authChain?: unknown;
  };
  return (
    typeof s === "object" &&
    s !== null &&
    typeof s.ephemeralIdentity === "object" &&
    s.ephemeralIdentity !== null &&
    typeof s.ephemeralIdentity.privateKey === "string" &&
    s.ephemeralIdentity.privateKey.length > 0 &&
    typeof s.expiration === "string" &&
    Array.isArray(s.authChain)
  );
};

const identity = (over: Record<string, unknown> = {}) => ({
  ephemeralIdentity: { address: "0xeee", privateKey: "0xkey" },
  expiration: "2099-01-01T00:00:00.000Z",
  authChain: [
    { type: "SIGNER", payload: "0xabc", signature: "" },
    { type: "ECDSA_EPHEMERAL", payload: "Decentraland Login", signature: "0xsig" },
  ],
  ...over,
});

describe("stored identity validation", () => {
  const cases: [string, unknown, boolean][] = [
    ["what this build writes", identity(), true],
    [
      "a writer that also stored the ephemeral public key",
      identity({
        ephemeralIdentity: { address: "0xeee", privateKey: "0xkey", publicKey: "0xpub" },
      }),
      true,
    ],
    [
      "auth link renamed its payload field",
      identity({ authChain: [{ type: "SIGNER", message: "0xabc", signature: "" }] }),
      false,
    ],
    [
      "auth link carries a link type this build does not know",
      identity({
        authChain: [{ type: "ECDSA_SIGNED_ENTITY_V2", payload: "x", signature: "y" }],
      }),
      false,
    ],
    [
      "auth link signature became a structured signature",
      identity({
        authChain: [{ type: "SIGNER", payload: "0xabc", signature: { r: "1", s: "2" } }],
      }),
      false,
    ],
    [
      "ephemeral address dropped",
      identity({ ephemeralIdentity: { privateKey: "0xkey" } }),
      false,
    ],
  ];
  for (const [name, value, shouldPass] of cases) {
    test(name, () => {
      expect(StoredAuthIdentitySchema.safeParse(value).success).toBe(shouldPass);
      // The guard that shipped before let every one of these through, valid or not.
      expect(oldIdentityGuard(value)).toBe(true);
    });
  }
});

const place = (over: Record<string, unknown> = {}) => ({
  id: "0,0",
  title: "Plaza",
  description: "",
  image: "https://catalyst.example/content/contents/bafy",
  coords: "0,0",
  x: 0,
  y: 0,
  left: 50,
  top: 50,
  players: null,
  live: false,
  featured: false,
  rating: 0,
  favorites: 0,
  likes: 0,
  visits: 0,
  parcels: 1,
  categories: [],
  creator: "Unknown creator",
  world: false,
  worldName: null,
  updated: "\u{2014}",
  hue: 12,
  kind: "place",
  ...over,
});

describe("recent places validation", () => {
  // `Array.isArray` was the whole of it, so a list of anything was a list of places.
  const oldRecentGuard = (v: unknown) => Array.isArray(v);

  const cases: [string, unknown, boolean][] = [
    ["a place this build wrote", [place()], true],
    ["a place with no image, which JSON drops rather than stores", [without(place(), "image")], true],
    ["an older build's list of ids", [{ id: "0,0" }], false],
    ["coords stored as a pair instead of a string", [place({ coords: [0, 0] })], false],
    [
      "worldName still under its API spelling",
      [{ ...without(place(), "worldName"), world_name: "shibuya.dcl.eth" }],
      false,
    ],
    [
      "categories became objects rather than names",
      [place({ categories: [{ name: "game" }] })],
      false,
    ],
  ];
  for (const [name, value, shouldPass] of cases) {
    test(name, () => {
      expect(RecentPlacesSchema.safeParse(value).success).toBe(shouldPass);
      expect(oldRecentGuard(value)).toBe(true);
    });
  }
});
