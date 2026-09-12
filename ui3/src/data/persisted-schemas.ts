
import { z } from "zod";

import type { StoredAuthIdentity } from "./auth/engineLogin";
import type { PlaceView } from "./catalyst/places";

export const AuthLinkSchema = z.looseObject({
  type: z.enum(["SIGNER", "ECDSA_EPHEMERAL", "ECDSA_SIGNED_ENTITY"]),
  payload: z.string(),
  signature: z.string(),
});

export const StoredAuthIdentitySchema = z.looseObject({
  ephemeralIdentity: z.looseObject({
    address: z.string(),
    privateKey: z.string(),
    publicKey: z.string().optional(),
  }),
  expiration: z.string(),
  authChain: z.array(AuthLinkSchema),
});

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

type UndefinedKeys<T> = { [K in keyof T]-?: undefined extends T[K] ? K : never }[keyof T];
type JsonRoundTrip<T> = Omit<T, UndefinedKeys<T>> & { [K in UndefinedKeys<T>]?: T[K] };

export type _AssertStoredAuthIdentity = Assert<
  Mutual<JsonRoundTrip<StoredAuthIdentity>, z.infer<typeof StoredAuthIdentitySchema>>
>;
export type _AssertRecentPlace = Assert<
  Mutual<JsonRoundTrip<PlaceView>, z.infer<typeof RecentPlaceSchema>>
>;
