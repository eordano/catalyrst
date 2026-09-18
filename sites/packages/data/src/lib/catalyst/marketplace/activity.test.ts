import { describe, expect, it } from "vitest";

import {
  buildFeed,
  formatMana,
  saleToEntry,
  shortAddr,
  tradeToEntry,
  type Sale,
  type Trade,
} from "./activity";
import fixture from "../../../fixtures/marketplace-activity.json";

const sales = fixture.sales.data as unknown as Sale[];
const trades = fixture.trades.data as unknown as Trade[];

describe("formatMana", () => {
  it("converts wei to MANA and treats 0 / empty / null / the MAX_UINT256 sentinel as unpriced", () => {
    expect(formatMana("20000000000000000000")).toBe("20");
    expect(formatMana("100000000000000000000")).toBe("100");
    expect(formatMana("0")).toBeNull();
    expect(formatMana("")).toBeNull();
    expect(formatMana(null)).toBeNull();
    const maxUint256 = (2n ** 256n - 1n).toString();
    expect(formatMana(maxUint256)).toBeNull();
    expect(formatMana((2n ** 248n).toString())).toBeNull();
  });
});

describe("shortAddr", () => {
  it("shortens a long address to 0xABCD\u{2026}1234 and returns empty for missing addresses", () => {
    expect(shortAddr("0x6042a0368f7bf354b44aab2fad70c4977dc24afb")).toBe(
      "0x6042\u{2026}4afb",
    );
    expect(shortAddr(null)).toBe("");
    expect(shortAddr(undefined)).toBe("");
  });
});

describe("saleToEntry / tradeToEntry", () => {
  it("maps a sale and a trade (no price, derived kind) onto ActivityEntry", () => {
    const e = saleToEntry({
      id: "sale-1",
      type: "order",
      price: "20000000000000000000",
      seller: "0x6042a0368f7bf354b44aab2fad70c4977dc24afb",
      buyer: "0x1111a0368f7bf354b44aab2fad70c4977dc22222",
      network: "ETHEREUM",
      timestamp: 1_700_000_000_000,
      txHash: "0xabc",
      contractAddress: "0xcol",
    } as unknown as Sale);
    expect(e).toMatchObject({
      id: "sale-1",
      kind: "sale",
      price: "20",
      network: "ethereum",
      timestamp: 1_700_000_000_000,
      txHash: "0xabc",
    });
    expect(e.from).toMatch(/^0x6042.*4afb$/);
    expect(e.to).toMatch(/^0x1111.*2222$/);

    const te = tradeToEntry({
      id: "trade-1",
      type: "bid",
      network: "MATIC",
      signer: "0x6042a0368f7bf354b44aab2fad70c4977dc24afb",
      created_at: "2026-01-02T00:00:00.000Z",
      contract: "0xcol",
    } as unknown as Trade);
    expect(te).toMatchObject({
      id: "trade-1",
      kind: "bid",
      price: null,
      network: "polygon",
      timestamp: Date.UTC(2026, 0, 2),
    });
    expect(te.from).toMatch(/^0x6042.*4afb$/);
    expect(tradeToEntry({ id: "trade-2", type: "public_nft_order" } as unknown as Trade)).toMatchObject({
      kind: "listing",
      timestamp: 0,
    });
  });
});

describe("buildFeed", () => {
  it("merges sales + trades newest-first and narrows the feed to a single kind", () => {
    const feed = buildFeed(sales, trades);
    expect(feed.length).toBe(sales.length + trades.length);
    for (let i = 1; i < feed.length; i++) {
      expect(feed[i - 1].timestamp).toBeGreaterThanOrEqual(feed[i].timestamp);
    }

    const onlySales = buildFeed(sales, trades, "sale");
    expect(onlySales.every((e) => e.kind === "sale")).toBe(true);

    const onlyListings = buildFeed(sales, trades, "listing");
    expect(onlyListings.every((e) => e.kind === "listing")).toBe(true);
    expect(onlyListings.length).toBeLessThanOrEqual(trades.length);

    const onlyBids = buildFeed(sales, trades, "bid");
    expect(onlyBids.every((e) => e.kind === "bid")).toBe(true);
  });
});
