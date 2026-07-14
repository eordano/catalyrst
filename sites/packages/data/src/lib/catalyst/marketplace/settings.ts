import { z } from "zod";

export function getStoreUrn(address: string): string {
  return `urn:decentraland:off-chain:marketplace-stores:${address.toLowerCase()}`;
}

const nullableStr = z
  .string()
  .nullish()
  .transform((v) => v ?? null);

/**
 * The store editor's form model, not a parse result. Empty strings here mean
 * "this input is blank", which is what an unset store field renders as; the
 * wire shape is `StoreEntitySchema`, where absence stays `null`.
 */
export type Store = {
  owner: string;
  cover: string;
  coverName: string;
  description: string;
  website: string;
  facebook: string;
  twitter: string;
  discord: string;
};

export const AuthorizationSchema = z.object({
  id: z.string(),
  contract: z.string(),
  address: nullableStr.optional(),
  token: z.string().nullish().transform((v) => v ?? undefined),
  network: z.string(),
  granted: z.boolean(),
  pending: z.boolean(),
});
export type Authorization = z.infer<typeof AuthorizationSchema>;

export const AuthorizationsSchema = z.object({
  buying: z.array(AuthorizationSchema),
  bidding: z.array(AuthorizationSchema),
  renting: z.array(AuthorizationSchema),
  selling: z.array(AuthorizationSchema),
});
export type Authorizations = z.infer<typeof AuthorizationsSchema>;

/**
 * The deployed store entity's metadata. `links` and `images` are always written
 * (empty when the seller set none) by every client that publishes this pointer,
 * so metadata without them is not a store with no links -- it is not a store,
 * and `loadStore` reports it as unavailable rather than showing the seller a
 * blank form that a save would deploy over their real settings.
 */
export const StoreEntitySchema = z.object({
  id: z.string(),
  owner: z.string(),
  description: nullableStr,
  links: z.array(z.object({ name: z.string(), url: z.string() })),
  images: z.array(z.object({ name: z.string(), file: z.string() })),
  version: z.number().nullish().transform((v) => v ?? null),
});
export type StoreEntity = z.infer<typeof StoreEntitySchema>;

export function emptyStore(owner = ""): Store {
  return {
    owner,
    cover: "",
    coverName: "",
    description: "",
    website: "",
    facebook: "",
    twitter: "",
    discord: "",
  };
}

export function emptyAuthorizations(): Authorizations {
  return { buying: [], bidding: [], renting: [], selling: [] };
}

export function storeFromEntity(entity: StoreEntity, peerBase: string): Store {
  const link = (name: string) =>
    entity.links.find((l) => l.name === name)?.url ?? "";
  const cover = entity.images.find((i) => i.name === "cover");
  return {
    owner: entity.owner,
    cover: cover ? `${peerBase}/content/contents/${cover.file}` : "",
    coverName: cover?.file ?? "",
    description: entity.description ?? "",
    website: link("website"),
    facebook: link("facebook"),
    twitter: link("twitter"),
    discord: link("discord"),
  };
}

export function toSellingRows(auth: Authorizations) {
  return auth.selling.map((s) => ({
    id: s.id,
    contract: s.contract,
    token: s.token ?? "Wearables",
    network: s.network,
  }));
}

export function grantedCount(auth: Authorizations): number {
  return [auth.buying, auth.bidding, auth.renting, auth.selling]
    .flat()
    .filter((a) => a.granted).length;
}
