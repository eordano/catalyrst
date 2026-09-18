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
  it("maps a fixture sale and a fixture trade (no price, derived kind) onto ActivityEntry", () => {
    const s = sales[0];
    const e = saleToEntry(s);
    expect(e.id).toBe(s.id);
    expect(e.kind).toBe(s.type === "bid" ? "bid" : "sale");
    expect(e.price).toBe(formatMana(s.price));
    expect(e.from).toBe(shortAddr(s.seller));
    expect(e.to).toBe(shortAddr(s.buyer));
    expect(e.network).toBe(s.network === "ETHEREUM" ? "ethereum" : "polygon");
    expect(e.txHash).toBe(s.txHash);

    const t = trades[0];
    const te = tradeToEntry(t);
    expect(te.id).toBe(t.id);
    expect(te.kind).toBe(t.type === "bid" ? "bid" : "listing");
    expect(te.price).toBeNull();
    expect(te.from).toBe(shortAddr(t.signer));
    expect(te.timestamp).toBe(t.created_at ? Date.parse(t.created_at) : 0);
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
