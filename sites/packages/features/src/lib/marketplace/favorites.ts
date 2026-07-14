import { check } from "@ui/validate";
import type { ShopCard } from "@ui/marketplace/new-shop/NewShopHome";

import { getIdentity, subscribe as subscribeSession } from "@data/lib/auth/session";
import { PersistedFavoritesSchema } from "@data/lib/persisted-schemas";
import type { CollectibleCard } from "@data/lib/catalyst/marketplace/index";

const KEY_PREFIX = "dcl:shop:favorites:";
const LEGACY_KEY = "dcl:shop:favorites";

function activeKey(): string | null {
  const signer = getIdentity()?.signer;
  return signer ? KEY_PREFIX + signer.toLowerCase() : null;
}

export function collectibleToShopCard(c: CollectibleCard): ShopCard {
  return {
    id: c.id,
    name: c.name,
    meta: c.collection ?? "Collectible",
    price: c.credits ?? c.price ?? undefined,
    unit: c.credits != null ? "credits" : "mana",
    rarity: c.rarity,
    network: c.network,
    image: c.image,
  };
}
const listeners = new Set<() => void>();

/**
 * The try covers the storage calls and the parse only. `check` throws in dev by
 * design, and a catch wide enough to cover it would show an empty favourites
 * list instead -- the drift hidden behind the exact symptom it causes.
 *
 * The `unit` migration stays AFTER the read: cards written before `unit`
 * existed are valid, and defaulting them is what the schema deliberately does
 * not do.
 */
function read(): ShopCard[] {
  if (typeof window === "undefined") return [];
  let parsed: unknown;
  try {
    window.localStorage.removeItem(LEGACY_KEY);
    const key = activeKey();
    if (!key) return [];
    const raw = window.localStorage.getItem(key);
    parsed = raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const cards = check(PersistedFavoritesSchema, parsed, "persisted/shop-favorites");
  return cards.map((c) =>
    c && typeof c === "object" && c.unit == null ? { ...c, unit: "mana" } : c,
  );
}

function write(cards: ShopCard[]): void {
  if (typeof window === "undefined") return;
  try {
    const key = activeKey();
    if (key) window.localStorage.setItem(key, JSON.stringify(cards));
  } catch {
  }
  listeners.forEach((l) => l());
}

export function getFavorites(): ShopCard[] {
  return read();
}

export function isFavorite(id: string): boolean {
  return read().some((c) => c.id === id);
}

export function toggleFavorite(card: ShopCard): boolean {
  const cur = read();
  const has = cur.some((c) => c.id === card.id);
  write(has ? cur.filter((c) => c.id !== card.id) : [card, ...cur]);
  return !has;
}

export function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  const onStorage = (e: StorageEvent) => {
    if (e.key && e.key.startsWith(KEY_PREFIX)) cb();
  };
  const unsubSession = subscribeSession(cb);
  if (typeof window !== "undefined") window.addEventListener("storage", onStorage);
  return () => {
    listeners.delete(cb);
    unsubSession();
    if (typeof window !== "undefined") window.removeEventListener("storage", onStorage);
  };
}
