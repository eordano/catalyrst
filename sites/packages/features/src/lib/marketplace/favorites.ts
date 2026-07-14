import type { ShopCard } from "@ui/marketplace/new-shop/NewShopHome";

import { getIdentity, subscribe as subscribeSession } from "@data/lib/auth/session";
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

function read(): ShopCard[] {
  if (typeof window === "undefined") return [];
  try {
    window.localStorage.removeItem(LEGACY_KEY);
    const key = activeKey();
    if (!key) return [];
    const raw = window.localStorage.getItem(key);
    const arr = raw ? JSON.parse(raw) : [];
    if (!Array.isArray(arr)) return [];
    return (arr as ShopCard[]).map((c) =>
      c && typeof c === "object" && c.unit == null ? { ...c, unit: "mana" } : c,
    );
  } catch {
    return [];
  }
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
