import type { z } from "zod";

import { postJSON, signedGetJSON } from "../client";
import type { AuthIdentity } from "../../auth/types";

import type { CartOut as RsCartOut } from "@ui/generated/catalyst/credits/CartOut";
import type { CartLineOut as RsCartLineOut } from "@ui/generated/catalyst/credits/CartLineOut";
import { CartLineOutSchema, CartOutSchema } from "../generated-schemas/credits";

export const CartLineSchema = CartLineOutSchema;
export type CartLine = z.infer<typeof CartLineSchema>;

export const CartSchema = CartOutSchema;
export type Cart = z.infer<typeof CartSchema>;

export function parseCart(raw: unknown): Cart {
  return CartSchema.parse(raw);
}

export async function fetchCart(
  identity: AuthIdentity,
  signal?: AbortSignal,
): Promise<Cart> {
  const raw = await signedGetJSON<unknown>("/credits/cart", {
    identity,
    signal,
  });
  return parseCart(raw);
}

export type ItemRef = { collection: string; itemId: string };

export function parseItemRef(raw: string): ItemRef | null {
  const s = raw.trim();
  const m = /^(0x[0-9a-fA-F]{40})-(.+)$/.exec(s);
  if (!m) return null;
  const itemId = m[2].trim();
  if (!itemId) return null;
  return { collection: m[1].toLowerCase(), itemId };
}

export async function addCartItem(
  identity: AuthIdentity,
  ref: ItemRef,
  qty?: number,
  signal?: AbortSignal,
): Promise<Cart> {
  const body: { itemId: string; collection: string; qty?: number } = {
    itemId: ref.itemId,
    collection: ref.collection,
  };
  if (qty !== undefined) body.qty = qty;
  const raw = await postJSON<unknown>("/credits/cart/items", body, {
    identity,
    signal,
  });
  return parseCart(raw);
}

export async function removeCartItem(
  identity: AuthIdentity,
  collection: string,
  itemId: string,
  signal?: AbortSignal,
): Promise<Cart> {
  const raw = await postJSON<unknown>(
    `/credits/cart/items/${encodeURIComponent(
      collection,
    )}/${encodeURIComponent(itemId)}`,
    undefined,
    { identity, method: "DELETE", signal },
  );
  return parseCart(raw);
}

export function cartCount(cart: Cart): number {
  return cart.items.reduce((n, i) => n + i.qty, 0);
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;

export type _DriftCartLine = Assert<
  AssignableTo<RsCartLineOut, z.input<typeof CartLineSchema>>
>;
export type _DriftCart = Assert<
  AssignableTo<RsCartOut, z.input<typeof CartSchema>>
>;
