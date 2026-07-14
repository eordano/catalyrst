import { describe, expect, it } from "vitest";

import fixture from "../../../fixtures/marketplace-cancel-listing.json";
import {
  fetchOrders,
  parseOrders,
  toCancelListing,
  formatOrderMana,
  orderNetwork,
  shortHex,
  OrderSchema,
} from "./orders";

const ROWS = (fixture as { orders: unknown[] }).orders;

describe("OrderSchema / parseOrders", () => {
  it("accepts the captured fixture order rows", () => {
    for (const row of ROWS) {
      expect(OrderSchema.safeParse(row).success).toBe(true);
    }
    const parsed = parseOrders(ROWS);
    expect(parsed.length).toBe(ROWS.length);
    expect(parsed[0].status).toBe("open");
    expect(parsed[0].owner).toMatch(/^0x/);
  });
});

describe("formatOrderMana", () => {
  it("formats wei -> human MANA", () => {
    expect(formatOrderMana("1000000000000000000")).toBe("1");
    expect(formatOrderMana("0")).toBe("0");
    expect(formatOrderMana("2500000000000000000")).toBe("2.5");
  });

  it("says nothing when there is no readable price", () => {
    expect(formatOrderMana(null)).toBeNull();
    expect(formatOrderMana("")).toBeNull();
    expect(formatOrderMana("not-a-number")).toBeNull();
  });
});

describe("orderNetwork / shortHex", () => {
  it("maps the catalyst token and shortens hashes", () => {
    expect(orderNetwork("ETHEREUM")).toBe("ethereum");
    expect(orderNetwork("MATIC")).toBe("polygon");
    expect(orderNetwork(null)).toBe("polygon");
    expect(shortHex("0x6ae4b880dad7bc413a256447d59eeac51ad8fa6225ea1e4722cb73c33fcc0cb1")).toBe(
      "0x6ae4\u{2026}0cb1",
    );
    expect(shortHex("")).toBe("");
  });
});

describe("toCancelListing", () => {
  it("projects an order row onto the cancel view-model", () => {
    const vm = toCancelListing(parseOrders(ROWS)[0]);
    expect(vm.orderId).toBe(
      "0x6ae4b880dad7bc413a256447d59eeac51ad8fa6225ea1e4722cb73c33fcc0cb1",
    );
    expect(vm.price).toBe("1");
    expect(vm.network).toBe("polygon");
    expect(vm.issuedId).toBe("47828");
    expect(vm.name).toBe("Listing #47828");
  });
});

describe("fetchOrders (injected fetch, no network)", () => {
  it("unwraps the {data,total} envelope and validates rows", async () => {
    const stub: typeof fetch = async () =>
      new Response(JSON.stringify({ data: ROWS, total: 91372 }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    const env = await fetchOrders(
      { owner: "0x00009dc8aac69accf38e87ab42a82a28be68f2a0", status: "open" },
      { fetchImpl: stub },
    );
    expect(env.total).toBe(91372);
    expect(env.data.length).toBe(ROWS.length);
    expect(env.data[0].status).toBe("open");
  });

  it("passes the composite itemId filter through to the query string", async () => {
    const urls: string[] = [];
    const stub: typeof fetch = async (input) => {
      urls.push(String(input));
      return new Response(JSON.stringify({ data: [], total: 0 }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    };
    const contract = "0xbb7f0ab8123be56dfc8e8a1e49150687fae36583";
    await fetchOrders(
      { contractAddress: contract, itemId: `${contract}-1`, status: "open", first: 24 },
      { fetchImpl: stub },
    );
    expect(urls).toHaveLength(1);
    const url = new URL(urls[0]);
    expect(url.searchParams.get("itemId")).toBe(`${contract}-1`);
    expect(url.searchParams.get("contractAddress")).toBe(contract);
    expect(url.searchParams.get("tokenId")).toBeNull();
  });
});
