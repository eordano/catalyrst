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
  it("converts a wei string to a human MANA amount", () => {
    expect(formatMana("20000000000000000000")).toBe("20");
    expect(formatMana("100000000000000000000")).toBe("100");
  });
  it("treats 0 / empty / null as unpriced", () => {
    expect(formatMana("0")).toBeNull();
    expect(formatMana("")).toBeNull();
    expect(formatMana(null)).toBeNull();
  });
  it("treats the on-chain MAX_UINT256 'not for sale' sentinel as unpriced", () => {
    const maxUint256 = (2n ** 256n - 1n).toString();
    expect(formatMana(maxUint256)).toBeNull();
    expect(formatMana((2n ** 248n).toString())).toBeNull();
    expect(formatMana("20000000000000000000")).toBe("20");
  });
});

describe("shortAddr", () => {
  it("shortens a long address to 0xABCD\u{2026}1234", () => {
    expect(shortAddr("0x6042a0368f7bf354b44aab2fad70c4977dc24afb")).toBe(
      "0x6042\u{2026}4afb",
    );
  });
  it("returns empty string for missing addresses", () => {
    expect(shortAddr(null)).toBe("");
    expect(shortAddr(undefined)).toBe("");
  });
});

describe("saleToEntry", () => {
  it("maps a fixture sale onto an ActivityEntry", () => {
    const s = sales[0];
    const e = saleToEntry(s);
    expect(e.id).toBe(s.id);
    expect(e.kind).toBe(s.type === "bid" ? "bid" : "sale");
    expect(e.price).toBe(formatMana(s.price));
    expect(e.from).toBe(shortAddr(s.seller));
    expect(e.to).toBe(shortAddr(s.buyer));
    expect(e.network).toBe(s.network === "ETHEREUM" ? "ethereum" : "polygon");
    expect(e.txHash).toBe(s.txHash);
  });
});

describe("tradeToEntry", () => {
  it("maps a fixture trade onto an ActivityEntry (no price, derived kind)", () => {
    const t = trades[0];
    const e = tradeToEntry(t);
    expect(e.id).toBe(t.id);
    expect(e.kind).toBe(t.type === "bid" ? "bid" : "listing");
    expect(e.price).toBeNull();
    expect(e.from).toBe(shortAddr(t.signer));
    expect(e.timestamp).toBe(t.created_at ? Date.parse(t.created_at) : 0);
  });
});

describe("buildFeed", () => {
  it("merges sales + trades into one feed sorted newest-first", () => {
    const feed = buildFeed(sales, trades);
    expect(feed.length).toBe(sales.length + trades.length);
    for (let i = 1; i < feed.length; i++) {
      expect(feed[i - 1].timestamp).toBeGreaterThanOrEqual(feed[i].timestamp);
    }
  });

  it("narrows the feed to a single kind", () => {
    const onlySales = buildFeed(sales, trades, "sale");
    expect(onlySales.every((e) => e.kind === "sale")).toBe(true);

    const onlyListings = buildFeed(sales, trades, "listing");
    expect(onlyListings.every((e) => e.kind === "listing")).toBe(true);
    expect(onlyListings.length).toBeLessThanOrEqual(trades.length);

    const onlyBids = buildFeed(sales, trades, "bid");
    expect(onlyBids.every((e) => e.kind === "bid")).toBe(true);
  });
});
