// Shapes this app writes into browser storage and reads back later.
//
// Hand-written, unlike the modules under src/generated: nothing on this
// boundary has a Rust source. `StoredAuthIdentity` and `PlaceView` are
// TypeScript shapes owned here, so a generator would have nothing to generate
// from -- the mutual asserts at the bottom stand in for one, failing typecheck
// the day either shape changes without the schema following.
//
// Everything here is a schema and nothing here is logic, which is what lets the
// perf build alias the whole module away (vite.validate.js).
//

// `z.looseObject`, not `z.object`: zod STRIPS unknown keys, and these values are
// round-tripped. A persisted blob is read, modified and written back, so a
// stripping parse permanently deletes any field this build does not know about
// -- a newer build's field, erased by an older one during a partial rollout.
// Verified against pending-store, which reads through `check` and writes the
// result straight back. External responses get the same treatment because they
// are forwarded onward and the upstream is free to add fields.
//
// Transforms and coercion still apply; loose only changes what happens to keys
// the shape does not mention.
import { z } from "zod";

import type { StoredAuthIdentity } from "./auth/engineLogin";
import type { PlaceView } from "./catalyst/places";

/** `signature` is `""` on the SIGNER link, which is how the chain is built. */
export const AuthLinkSchema = z.looseObject({
  type: z.enum(["SIGNER", "ECDSA_EPHEMERAL", "ECDSA_SIGNED_ENTITY"]),
  payload: z.string(),
  signature: z.string(),
});

/**
 * `dcl-auth-identity` -- the wallet session, and the highest-consequence value
 * on this boundary: it is what gets handed to the signing path.
 *
 * `publicKey` is optional because `toStoredIdentity` does not write one; a blob
 * that carries it came from a different writer and is still usable.
 */
export const StoredAuthIdentitySchema = z.looseObject({
  ephemeralIdentity: z.looseObject({
    address: z.string(),
    privateKey: z.string(),
    publicKey: z.string().optional(),
  }),
  expiration: z.string(),
  authChain: z.array(AuthLinkSchema),
});

/** `dcl.recentPlaces` -- cached `toPlaceView` output, one entry per visit. */
export const RecentPlaceSchema = z.looseObject({
  id: z.string(),
  title: z.string(),
  description: z.string(),
  image: z.string().optional(),
  coords: z.string(),
  x: z.number(),
  y: z.number(),
  left: z.number(),
  top: z.number(),
  players: z.number().nullable(),
  live: z.boolean(),
  featured: z.boolean(),
  rating: z.number(),
  favorites: z.number(),
  likes: z.number(),
  visits: z.number(),
  parcels: z.number(),
  categories: z.array(z.string()),
  creator: z.string(),
  world: z.boolean(),
  worldName: z.string().nullable(),
  updated: z.string(),
  hue: z.number(),
  kind: z.string(),
});

export const RecentPlacesSchema = z.array(RecentPlaceSchema);

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

/**
 * A key whose value may be `undefined` comes back ABSENT, because
 * `JSON.stringify` drops it. Normalising the in-memory type to what storage can
 * actually hold is what lets the assert below compare the parts that differ
 * instead of failing on that one, unavoidable, difference.
 */
type UndefinedKeys<T> = { [K in keyof T]-?: undefined extends T[K] ? K : never }[keyof T];
type JsonRoundTrip<T> = Omit<T, UndefinedKeys<T>> & { [K in UndefinedKeys<T>]?: T[K] };

export type _AssertStoredAuthIdentity = Assert<
  Mutual<JsonRoundTrip<StoredAuthIdentity>, z.infer<typeof StoredAuthIdentitySchema>>
>;
export type _AssertRecentPlace = Assert<
  Mutual<JsonRoundTrip<PlaceView>, z.infer<typeof RecentPlaceSchema>>
>;
