
import { z } from "zod";

import type { ShopCard } from "@ui/marketplace/new-shop/NewShopHome";

import type { AuthIdentity } from "./auth/types";
import type { ThirdwebSession } from "./auth/thirdweb/session";
import type { SimDraftFile, SimStore } from "./catalyst/builder/sim-collection-items";
import type { PendingCheckout } from "./catalyst/marketplace/pending-checkout";
import type { PendingTopup } from "./catalyst/marketplace/pending-topup";

export const AuthLinkSchema = z.looseObject({
  type: z.enum(["SIGNER", "ECDSA_EPHEMERAL", "ECDSA_SIGNED_ENTITY"]),
  payload: z.string(),
  signature: z.string(),
});

const HexKeySchema = z.custom<`0x${string}`>(
  (v) => typeof v === "string" && /^0x[0-9a-fA-F]{64}$/.test(v),
  { message: "expected a 0x-prefixed 32-byte hex key" },
);

export const AuthIdentitySchema = z.looseObject({
  signer: z.string(),
  ephemeral: z.looseObject({
    address: z.string(),
    privateKey: HexKeySchema,
  }),
  expiration: z.string(),
  authChain: z.array(AuthLinkSchema),
});

export const ThirdwebSessionSchema = z.looseObject({
  token: z.string(),
  address: z.string(),
});

export const PendingTopupStoreSchema = z.record(
  z.string(),
  z.looseObject({ txHash: z.string(), ts: z.number() }),
);

export const PendingCheckoutStoreSchema = z.record(
  z.string(),
  z.looseObject({ checkoutId: z.number(), ts: z.number() }),
);

export const SimDraftFileSchema = z.looseObject({
  name: z.string(),
  size: z.number(),
  fileType: z.string(),
});

export const SimCollectionItemsStoreSchema = z.record(
  z.string(),
  z.looseObject({ ts: z.number(), files: z.array(SimDraftFileSchema) }),
);

export const DevSignerKeySchema = z.string().regex(/^0x[0-9a-fA-F]{64}$/);

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

export type _AssertPersistedShopCard = Assert<
  AssignableTo<z.infer<typeof PersistedShopCardSchema>, ShopCard>
>;
