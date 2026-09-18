import { describe, expect, it } from "vitest";

import {
  catalogManaPrice,
  isCatalogItemBuyable,
  isEnsBuyable,
  type CatalogItem,
  type EnsResult,
} from "./index";

const WEI = "000000000000000000";

function item(over: Partial<CatalogItem>): CatalogItem {
  return {
    isOnSale: false,
    price: "0",
    minListingPrice: null,
    ...over,
  } as CatalogItem;
}

describe("catalogManaPrice \u{2014} upstream free-mint + listing-floor semantics", () => {
  it("free mint \u{2192} '0', paid mint \u{2192} mint price, listing floor wins, listed-only \u{2192} floor, nothing \u{2192} null", () => {
    expect(catalogManaPrice(item({ isOnSale: true, price: "0" }))).toBe("0");
    expect(catalogManaPrice(item({ isOnSale: true, price: `2${WEI}` }))).toBe("2");
    expect(
      catalogManaPrice(
        item({ isOnSale: true, price: `2${WEI}`, minListingPrice: `140000000000000000` }),
      ),
    ).toBe("0.14");
    expect(
      catalogManaPrice(item({ isOnSale: false, minListingPrice: `1800000000000000000` })),
    ).toBe("1.8");
    expect(catalogManaPrice(item({ isOnSale: false }))).toBeNull();
  });
});

describe("isCatalogItemBuyable \u{2014} browse surfaces only show what can be bought", () => {
  it("free mint and no-listing items are not buyable; paid mints, listing floors are; garbage prices are not", () => {
    expect(isCatalogItemBuyable(item({ isOnSale: true, price: "0" }))).toBe(false);
    expect(isCatalogItemBuyable(item({ isOnSale: true, price: `2${WEI}` }))).toBe(true);
    expect(
      isCatalogItemBuyable(item({ isOnSale: false, minListingPrice: `1${WEI}` })),
    ).toBe(true);
    expect(isCatalogItemBuyable(item({ isOnSale: false }))).toBe(false);
    expect(isCatalogItemBuyable(item({ isOnSale: true, price: "nope" }))).toBe(false);
  });
});

function ens(order: EnsResult["order"]): EnsResult {
  return { nft: { id: "x" }, order, rental: null } as unknown as EnsResult;
}

describe("isEnsBuyable", () => {
  it("only an open order with a positive price is buyable", () => {
    expect(isEnsBuyable(ens({ price: `5${WEI}`, status: "open" } as EnsResult["order"]))).toBe(true);
    expect(isEnsBuyable(ens({ price: "0", status: "open" } as EnsResult["order"]))).toBe(false);
    expect(isEnsBuyable(ens({ price: `5${WEI}`, status: "sold" } as EnsResult["order"]))).toBe(false);
    expect(isEnsBuyable(ens(null))).toBe(false);
  });
});
