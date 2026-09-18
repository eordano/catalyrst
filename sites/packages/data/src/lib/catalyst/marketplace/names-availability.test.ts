import { describe, expect, it } from "vitest";

import { ensAvailability, ensOrderExpired } from "./names";
import type { EnsResult } from "./index";

const WEI = "000000000000000000";
const NOW = 1_800_000_000_000;

function ens(over: {
  name?: string;
  tokenId?: string | null;
  order?: Partial<NonNullable<EnsResult["order"]>> | null;
}): EnsResult {
  return {
    nft: {
      id: "0xreg-1",
      contractAddress: "0xreg",
      tokenId: over.tokenId === undefined ? "1" : over.tokenId,
      name: over.name ?? "Automotive",
      data: { ens: { subdomain: over.name ?? "Automotive" } },
    },
    order: over.order === undefined ? null : over.order,
    rental: null,
  } as unknown as EnsResult;
}

function openOrder(over: Partial<NonNullable<EnsResult["order"]>> = {}) {
  return {
    id: "o1",
    status: "open",
    price: `5${WEI}`,
    expiresAt: NOW / 1000 + 3600,
    ...over,
  } as NonNullable<EnsResult["order"]>;
}

describe("ensOrderExpired \u{2014} mixed seconds/ms expiries", () => {
  it("treats small values as seconds, large values as milliseconds, and a missing expiry as never expiring", () => {
    expect(ensOrderExpired(NOW / 1000 - 60, NOW)).toBe(true);
    expect(ensOrderExpired(NOW / 1000 + 60, NOW)).toBe(false);
    expect(ensOrderExpired(NOW - 1, NOW)).toBe(true);
    expect(ensOrderExpired(NOW + 1, NOW)).toBe(false);
    expect(ensOrderExpired(null, NOW)).toBe(false);
    expect(ensOrderExpired(0, NOW)).toBe(false);
  });
});

describe("ensAvailability \u{2014} three honest states", () => {
  it("no minted NFT \u{2192} claimable; minted with no order \u{2192} taken", () => {
    expect(ensAvailability("fresh", [], "42", NOW)).toEqual({
      kind: "claimable",
      name: "fresh",
    });
    expect(ensAvailability("automotive", [ens({ order: null })], "1", NOW)).toEqual({
      kind: "taken",
      name: "Automotive",
    });
  });

  it("open order with price and future expiry \u{2192} listed with minted casing preserved, falling back to the computed tokenId when the NFT omits it", () => {
    const row = ens({ name: "Automotive", order: openOrder() });
    expect(ensAvailability("automotive", [row], "1", NOW)).toEqual({
      kind: "listed",
      name: "Automotive",
      contractAddress: "0xreg",
      tokenId: "1",
      priceWei: `5${WEI}`,
      priceMana: "5",
    });

    const noToken = ens({ tokenId: null, order: openOrder() });
    const res = ensAvailability("automotive", [noToken], "9000", NOW);
    expect(res.kind).toBe("listed");
    if (res.kind === "listed") expect(res.tokenId).toBe("9000");
  });

  it("an open order at zero price, an expired open order, or a cancelled order \u{2192} taken, never buyable", () => {
    expect(
      ensAvailability("automotive", [ens({ order: openOrder({ price: "0" }) })], "1", NOW).kind,
    ).toBe("taken");
    expect(
      ensAvailability(
        "automotive",
        [ens({ order: openOrder({ expiresAt: NOW / 1000 - 3600 }) })],
        "1",
        NOW,
      ).kind,
    ).toBe("taken");
    expect(
      ensAvailability(
        "automotive",
        [ens({ order: openOrder({ status: "cancelled" }) })],
        "1",
        NOW,
      ).kind,
    ).toBe("taken");
  });
});
