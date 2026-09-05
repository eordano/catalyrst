// Shapes this app writes into browser storage and reads back later.
//
// Hand-written, unlike everything under catalyst/generated-schemas: none of
// these has a Rust source. They are our own TypeScript shapes, written by one
// build and read by the next, so a generator would have nothing to generate
// from -- the asserts at the bottom stand in for one, failing typecheck the day
// a shape changes without its schema following.
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

import type { ShopCard } from "@ui/marketplace/new-shop/NewShopHome";

import type { AuthIdentity } from "./auth/types";
import type { ThirdwebSession } from "./auth/thirdweb/session";
import type { SimDraftFile, SimStore } from "./catalyst/builder/sim-collection-items";
import type { PendingCheckout } from "./catalyst/marketplace/pending-checkout";
import type { PendingTopup } from "./catalyst/marketplace/pending-topup";

/** `signature` is `""` on the SIGNER link, which is how the chain is built. */
export const AuthLinkSchema = z.looseObject({
  type: z.enum(["SIGNER", "ECDSA_EPHEMERAL", "ECDSA_SIGNED_ENTITY"]),
  payload: z.string(),
  signature: z.string(),
});

// A 32-byte key, not merely something starting with 0x. The prefix test
// accepted "0x" and "0xzz", and this backs AuthIdentity.ephemeral.privateKey --
// the value the EIP-712 and signed-fetch paths sign with. The surviving hand
// guard downstream is truthiness-only, so it catches an ABSENT key and not a
// truncated one. DevSignerKeySchema below already had the right form.
const HexKeySchema = z.custom<`0x${string}`>(
  (v) => typeof v === "string" && /^0x[0-9a-fA-F]{64}$/.test(v),
  { message: "expected a 0x-prefixed 32-byte hex key" },
);

/**
 * `dcl:auth:identity:v1` -- the wallet session, and the highest-consequence
 * value on this boundary: it is what the EIP-712 and signed-fetch paths sign
 * with.
 */
export const AuthIdentitySchema = z.looseObject({
  signer: z.string(),
  ephemeral: z.looseObject({
    address: z.string(),
    privateKey: HexKeySchema,
  }),
  expiration: z.string(),
  authChain: z.array(AuthLinkSchema),
});

/** `dcl:auth:thirdweb:v1` -- the enclave-wallet bearer token and its address. */
export const ThirdwebSessionSchema = z.looseObject({
  token: z.string(),
  address: z.string(),
});

/**
 * `dcl:mk:pending-mana-topup:v1` and `dcl:mk:pending-checkout:v1`, keyed by
 * lowercased signer. Both hold money in flight: a drifted entry is a purchase
 * the UI stops tracking, so this is the boundary where silence costs the most.
 */
export const PendingTopupStoreSchema = z.record(
  z.string(),
  z.looseObject({ txHash: z.string(), ts: z.number() }),
);

export const PendingCheckoutStoreSchema = z.record(
  z.string(),
  z.looseObject({ checkoutId: z.number(), ts: z.number() }),
);

/** `dcl:ch:sim-collection-items:v1` -- draft file lists, keyed by collection. */
export const SimDraftFileSchema = z.looseObject({
  name: z.string(),
  size: z.number(),
  fileType: z.string(),
});

export const SimCollectionItemsStoreSchema = z.record(
  z.string(),
  z.looseObject({ ts: z.number(), files: z.array(SimDraftFileSchema) }),
);

/**
 * `dcl:auth:dev-signer-pk:v1` -- the burner key behind dev sign-in.
 *
 * Stricter than the `startsWith("0x")` guard it backs, deliberately: anything
 * else stored under this key is not a key, and `privateKeyToAccount` would
 * throw on it further down.
 */
export const DevSignerKeySchema = z.string().regex(/^0x[0-9a-fA-F]{64}$/);

/**
 * `dcl:shop:favorites:<signer>` -- the JSON-safe narrowing of `ShopCard`.
 *
 * `name`, `meta` and `price` are `ReactNode` in memory but can only survive
 * storage as a string or a number, so the schema describes what can actually
 * come back rather than what the type allows. `unit` stays optional because
 * cards written before it existed are migrated after the read.
 */
const ShopTextSchema = z.union([z.string(), z.number()]);

export const PersistedShopCardSchema = z.looseObject({
  id: z.string(),
  name: ShopTextSchema.optional(),
  meta: ShopTextSchema.optional(),
  price: ShopTextSchema.optional(),
  unit: z.enum(["mana", "credits"]).optional(),
  rarity: z.string().optional(),
  network: z.enum(["polygon", "ethereum"]).optional(),
  image: z.string().optional(),
});

export const PersistedFavoritesSchema = z.array(PersistedShopCardSchema);

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertAuthIdentity = Assert<
  Mutual<AuthIdentity, z.infer<typeof AuthIdentitySchema>>
>;
export type _AssertThirdwebSession = Assert<
  Mutual<ThirdwebSession, z.infer<typeof ThirdwebSessionSchema>>
>;
export type _AssertPendingTopupStore = Assert<
  Mutual<Record<string, PendingTopup>, z.infer<typeof PendingTopupStoreSchema>>
>;
export type _AssertPendingCheckoutStore = Assert<
  Mutual<Record<string, PendingCheckout>, z.infer<typeof PendingCheckoutStoreSchema>>
>;
export type _AssertSimDraftFile = Assert<
  Mutual<SimDraftFile, z.infer<typeof SimDraftFileSchema>>
>;
export type _AssertSimCollectionItemsStore = Assert<
  Mutual<SimStore, z.infer<typeof SimCollectionItemsStoreSchema>>
>;

/**
 * One direction only, unlike the others: the persisted card is a deliberate
 * narrowing of `ShopCard`, not a mirror of it. What has to hold is that a
 * parsed card is still a usable `ShopCard`.
 */
export type _AssertPersistedShopCard = Assert<
  AssignableTo<z.infer<typeof PersistedShopCardSchema>, ShopCard>
>;
