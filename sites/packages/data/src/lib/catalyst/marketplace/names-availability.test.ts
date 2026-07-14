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

describe("ensOrderExpired \u{2014} mixed seconds/ms expiries", () => {
  it("treats small values as seconds", () => {
    expect(ensOrderExpired(NOW / 1000 - 60, NOW)).toBe(true);
    expect(ensOrderExpired(NOW / 1000 + 60, NOW)).toBe(false);
  });

  it("treats large values as milliseconds", () => {
    expect(ensOrderExpired(NOW - 1, NOW)).toBe(true);
    expect(ensOrderExpired(NOW + 1, NOW)).toBe(false);
  });

  it("missing expiry never expires", () => {
    expect(ensOrderExpired(null, NOW)).toBe(false);
    expect(ensOrderExpired(0, NOW)).toBe(false);
  });
});

describe("ensAvailability \u{2014} three honest states", () => {
  it("no minted NFT \u{2192} claimable", () => {
    expect(ensAvailability("fresh", [], "42", NOW)).toEqual({
      kind: "claimable",
      name: "fresh",
    });
  });

  it("open order with price and future expiry \u{2192} listed, minted casing preserved", () => {
    const row = ens({
      name: "Automotive",
      order: {
        id: "o1",
        status: "open",
        price: `5${WEI}`,
        expiresAt: NOW / 1000 + 3600,
      } as NonNullable<EnsResult["order"]>,
    });
    expect(ensAvailability("automotive", [row], "1", NOW)).toEqual({
      kind: "listed",
      name: "Automotive",
      contractAddress: "0xreg",
      tokenId: "1",
      priceWei: `5${WEI}`,
      priceMana: "5",
    });
  });

  it("falls back to the computed tokenId when the NFT omits it", () => {
    const row = ens({
      tokenId: null,
      order: {
        id: "o1",
        status: "open",
        price: `5${WEI}`,
        expiresAt: NOW / 1000 + 3600,
      } as NonNullable<EnsResult["order"]>,
    });
    const res = ensAvailability("automotive", [row], "9000", NOW);
    expect(res.kind).toBe("listed");
    if (res.kind === "listed") expect(res.tokenId).toBe("9000");
  });

  it("minted with no order \u{2192} taken", () => {
    expect(ensAvailability("automotive", [ens({ order: null })], "1", NOW)).toEqual({
      kind: "taken",
      name: "Automotive",
    });
  });

  it("open order with zero price \u{2192} taken, never buyable", () => {
    const row = ens({
      order: {
        id: "o1",
        status: "open",
        price: "0",
        expiresAt: NOW / 1000 + 3600,
      } as NonNullable<EnsResult["order"]>,
    });
    expect(ensAvailability("automotive", [row], "1", NOW).kind).toBe("taken");
  });

  it("expired open order \u{2192} taken", () => {
    const row = ens({
      order: {
        id: "o1",
        status: "open",
        price: `5${WEI}`,
        expiresAt: NOW / 1000 - 3600,
      } as NonNullable<EnsResult["order"]>,
    });
    expect(ensAvailability("automotive", [row], "1", NOW).kind).toBe("taken");
  });

  it("cancelled order \u{2192} taken", () => {
    const row = ens({
      order: {
        id: "o1",
        status: "cancelled",
        price: `5${WEI}`,
        expiresAt: NOW / 1000 + 3600,
      } as NonNullable<EnsResult["order"]>,
    });
    expect(ensAvailability("automotive", [row], "1", NOW).kind).toBe("taken");
  });
});
