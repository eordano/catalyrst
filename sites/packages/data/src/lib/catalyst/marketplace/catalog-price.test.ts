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
  it("free primary mint (isOnSale, price 0, no listings) \u{2192} '0', not null", () => {
    expect(catalogManaPrice(item({ isOnSale: true, price: "0" }))).toBe("0");
  });

  it("paid primary mint \u{2192} the mint price", () => {
    expect(catalogManaPrice(item({ isOnSale: true, price: `2${WEI}` }))).toBe("2");
  });

  it("listing floor wins over the mint price (cheapest buy)", () => {
    expect(
      catalogManaPrice(
        item({ isOnSale: true, price: `2${WEI}`, minListingPrice: `140000000000000000` }),
      ),
    ).toBe("0.14");
  });

  it("not minting but listed (isOnSale false + listings) \u{2192} the listing floor", () => {
    expect(
      catalogManaPrice(item({ isOnSale: false, minListingPrice: `1800000000000000000` })),
    ).toBe("1.8");
  });

  it("nothing to buy (no mint, no listings) \u{2192} null (Make an offer)", () => {
    expect(catalogManaPrice(item({ isOnSale: false }))).toBeNull();
  });
});

describe("isCatalogItemBuyable \u{2014} browse surfaces only show what can be bought", () => {
  it("free primary mint (isOnSale, price 0, no listings) is NOT buyable", () => {
    expect(isCatalogItemBuyable(item({ isOnSale: true, price: "0" }))).toBe(false);
  });

  it("paid primary mint is buyable", () => {
    expect(isCatalogItemBuyable(item({ isOnSale: true, price: `2${WEI}` }))).toBe(true);
  });

  it("listing floor makes a non-minting item buyable", () => {
    expect(
      isCatalogItemBuyable(item({ isOnSale: false, minListingPrice: `1${WEI}` })),
    ).toBe(true);
  });

  it("no mint, no listings \u{2192} not buyable", () => {
    expect(isCatalogItemBuyable(item({ isOnSale: false }))).toBe(false);
  });

  it("garbage price strings \u{2192} not buyable", () => {
    expect(isCatalogItemBuyable(item({ isOnSale: true, price: "nope" }))).toBe(false);
  });
});

function ens(order: EnsResult["order"]): EnsResult {
  return { nft: { id: "x" }, order, rental: null } as unknown as EnsResult;
}

describe("isEnsBuyable", () => {
  it("open order with positive price \u{2192} buyable", () => {
    expect(isEnsBuyable(ens({ price: `5${WEI}`, status: "open" } as EnsResult["order"]))).toBe(true);
  });

  it("open order at price 0 \u{2192} not buyable", () => {
    expect(isEnsBuyable(ens({ price: "0", status: "open" } as EnsResult["order"]))).toBe(false);
  });

  it("non-open order \u{2192} not buyable", () => {
    expect(isEnsBuyable(ens({ price: `5${WEI}`, status: "sold" } as EnsResult["order"]))).toBe(false);
  });

  it("no order \u{2192} not buyable", () => {
    expect(isEnsBuyable(ens(null))).toBe(false);
  });
});
