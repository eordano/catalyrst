import { afterEach, beforeEach, expect, test } from "vitest";
import { setIdentity } from "@data/lib/auth/session";
import type { AuthIdentity } from "@data/lib/auth/types";
import { getFavorites, isFavorite, subscribe, toggleFavorite } from "./favorites";
import type { ShopCard } from "@ui/marketplace/new-shop/NewShopHome";

type WinStub = { window?: unknown; document?: unknown };
function stubWindow() {
  const store = new Map<string, string>();
  (globalThis as WinStub).window = {
    localStorage: {
      getItem: (k: string) => (store.has(k) ? store.get(k) : null),
      setItem: (k: string, v: string) => void store.set(k, v),
      removeItem: (k: string) => void store.delete(k),
    },
    addEventListener: () => {},
    removeEventListener: () => {},
  };
}

function signIn(signer: string) {
  setIdentity({ signer } as AuthIdentity);
}

beforeEach(() => {
  stubWindow();
  signIn("0xAbC0000000000000000000000000000000000001");
});
afterEach(() => {
  setIdentity(null);
  delete (globalThis as WinStub).window;
});

const CARD: ShopCard = { id: "urn:a:1", name: "Hat", price: "500", rarity: "rare", network: "polygon" };

test("toggle adds newest-first then removes, getFavorites + isFavorite reflect it, and subscribers fire per write until unsubscribed", () => {
  let n = 0;
  const off = subscribe(() => {
    n += 1;
  });

  expect(getFavorites()).toEqual([]);
  expect(toggleFavorite(CARD)).toBe(true);
  expect(isFavorite("urn:a:1")).toBe(true);
  expect(n).toBe(1);
  toggleFavorite({ ...CARD, id: "urn:b:2", name: "Boots" });
  expect(getFavorites().map((c) => c.id)).toEqual(["urn:b:2", "urn:a:1"]);
  expect(n).toBe(2);

  off();
  expect(toggleFavorite(CARD)).toBe(false);
  expect(isFavorite("urn:a:1")).toBe(false);
  expect(getFavorites().map((c) => c.id)).toEqual(["urn:b:2"]);
  expect(n).toBe(2);
});

test("favorites are isolated per account and hidden when signed out", () => {
  toggleFavorite(CARD);
  expect(isFavorite("urn:a:1")).toBe(true);

  signIn("0xDeF0000000000000000000000000000000000002");
  expect(getFavorites()).toEqual([]);
  toggleFavorite({ ...CARD, id: "urn:b:2", name: "Boots" });
  expect(getFavorites().map((c) => c.id)).toEqual(["urn:b:2"]);

  setIdentity(null);
  expect(getFavorites()).toEqual([]);

  signIn("0xabc0000000000000000000000000000000000001");
  expect(getFavorites().map((c) => c.id)).toEqual(["urn:a:1"]);
});

test("pre-namespacing global bucket is dropped, never adopted", () => {
  const w = (globalThis as { window: { localStorage: Storage } }).window;
  w.localStorage.setItem("dcl:shop:favorites", JSON.stringify([CARD]));
  expect(getFavorites()).toEqual([]);
  expect(w.localStorage.getItem("dcl:shop:favorites")).toBeNull();
});
