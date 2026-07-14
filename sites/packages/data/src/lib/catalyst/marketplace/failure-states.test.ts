import { afterEach, describe, expect, it, vi } from "vitest";

import { parseBuyOrder, fetchOrders, fetchCheapestOpenOrder, fetchOpenOrderForToken } from "./buy";
import { fetchReceivedBids } from "./bids";
import { loadReceivedBids } from "./bids.server";
import { loadCancelListing } from "./orders.server";
import { loadPacks } from "./packs.server";
import { loadStore } from "./settings.server";

// A full wire row: parseBuyOrder now validates against the generated Order
// schema, so the fixture must be shaped like the real payload.
const ORDER = {
  id: "0x6ae4b880dad7bc413a256447d59eeac51ad8fa62",
  marketplaceAddress: "0x480a0f4e360e8964e68858dd231c2922f1df45ef",
  contractAddress: "0xbb7f0ab8123be56dfc8e8a1e49150687fae36583",
  tokenId: "47828",
  owner: "0x00009dc8aac69accf38e87ab42a82a28be68f2a0",
  buyer: null,
  price: "1000000000000000000",
  status: "open",
  expiresAt: 1_782_604_800_000,
  createdAt: 1_782_345_600_000,
  updatedAt: 1_782_345_600_000,
  network: "ETHEREUM",
  chainId: 1,
  issuedId: "47828",
  tradeId: null,
};

// A full wire row: fetchReceivedBids now validates against the generated
// BidsEnvelope schema, so the fixture must be shaped like the real payload.
const BID = {
  id: "bid-1",
  bidder: "0xbidder",
  price: "1000000000000000000",
  createdAt: 1_782_345_600_000,
  updatedAt: 1_782_345_600_000,
  fingerprint: "0x",
  status: "open",
  seller: "0xseller",
  network: "ETHEREUM",
  chainId: 1,
  contractAddress: "0xbb7f0ab8123be56dfc8e8a1e49150687fae36583",
  expiresAt: 1_782_604_800_000,
  tokenId: "104",
};

function jsonStub(body: unknown): typeof fetch {
  return async () =>
    new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
}

const throwingStub: typeof fetch = async () => {
  throw new Error("connection refused");
};

const brokenStub: typeof fetch = async () => new Response("upstream on fire", { status: 500 });

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("parseBuyOrder", () => {
  it("returns the order when it validates", () => {
    expect(parseBuyOrder(ORDER)?.price).toBe("1000000000000000000");
  });

  it("returns null rather than casting an unvalidated row through to a purchase", () => {
    expect(parseBuyOrder({ id: "only-an-id" })).toBeNull();
    expect(parseBuyOrder({ ...ORDER, price: 12 })).toBeNull();
    expect(parseBuyOrder(null)).toBeNull();
  });
});

describe("fetchOrders", () => {
  it("counts the rows it had to drop instead of hiding them", async () => {
    const env = await fetchOrders(
      {},
      { fetchImpl: jsonStub({ data: [ORDER, { id: "bad" }], total: 2 }) },
    );
    expect(env.data).toHaveLength(1);
    expect(env.invalid).toBe(1);
  });
});

describe("order lookups distinguish 'none' from 'could not read'", () => {
  it("reports empty when the node answers with no listings", async () => {
    const res = await fetchCheapestOpenOrder("0xc", {
      fetchImpl: jsonStub({ data: [], total: 0 }),
    });
    expect(res).toMatchObject({ order: null, source: "empty" });
  });

  it("reports unavailable when the read throws", async () => {
    const res = await fetchCheapestOpenOrder("0xc", { fetchImpl: throwingStub });
    expect(res.source).toBe("unavailable");
    expect(res.order).toBeNull();
    expect(res.reason).toBeTruthy();
  });

  it("reports unavailable when every row failed validation", async () => {
    const res = await fetchCheapestOpenOrder("0xc", {
      fetchImpl: jsonStub({ data: [{ id: "bad" }], total: 1 }),
    });
    expect(res.source).toBe("unavailable");
  });

  it("returns the validated order when there is one", async () => {
    const res = await fetchCheapestOpenOrder("0xc", {
      fetchImpl: jsonStub({ data: [ORDER], total: 1 }),
    });
    expect(res.source).toBe("catalyst");
    expect(res.order?.id).toBe(ORDER.id);
  });

  it("applies the same three states to the per-token lookup", async () => {
    expect(
      (await fetchOpenOrderForToken("0xc", "1", { fetchImpl: throwingStub })).source,
    ).toBe("unavailable");
    expect(
      (await fetchOpenOrderForToken("0xc", "1", { fetchImpl: jsonStub({ data: [], total: 0 }) }))
        .source,
    ).toBe("empty");
  });
});

describe("fetchReceivedBids", () => {
  it("says unavailable when the bids read fails", async () => {
    const res = await fetchReceivedBids("0xseller", { fetchImpl: brokenStub });
    expect(res).toMatchObject({ bids: [], source: "unavailable" });
    expect(res.reason).toBeTruthy();
  });

  it("says unavailable when the envelope carries no payload", async () => {
    const res = await fetchReceivedBids("0xseller", { fetchImpl: jsonStub({ ok: false }) });
    expect(res.source).toBe("unavailable");
  });

  it("says unavailable when no returned bid could be validated", async () => {
    const res = await fetchReceivedBids("0xseller", {
      fetchImpl: jsonStub({ ok: true, data: { results: [{ nope: 1 }], total: 1 } }),
    });
    expect(res.source).toBe("unavailable");
  });

  it("says empty only when the seller genuinely has no bids", async () => {
    const res = await fetchReceivedBids("0xseller", {
      fetchImpl: jsonStub({
        ok: true,
        data: { results: [], total: 0, page: 0, pages: 0, limit: 24 },
      }),
    });
    expect(res).toMatchObject({ bids: [], source: "empty" });
  });

  it("says live when there are bids", async () => {
    const res = await fetchReceivedBids("0xseller", {
      fetchImpl: jsonStub({
        ok: true,
        data: { results: [BID], total: 1, page: 0, pages: 1, limit: 24 },
      }),
    });
    expect(res.source).toBe("live");
    expect(res.bids).toHaveLength(1);
  });
});

describe("loadReceivedBids", () => {
  it("forwards the unavailable state to the accept-bid loader", async () => {
    const res = await loadReceivedBids("0xseller", { fetchImpl: brokenStub });
    expect(res.source).toBe("unavailable");
  });

  it("is empty when there is no signed-in owner to ask about", async () => {
    expect((await loadReceivedBids(null)).source).toBe("empty");
  });
});

describe("loadCancelListing", () => {
  it("says unavailable rather than 'no active listing' when the read fails", async () => {
    const res = await loadCancelListing({
      owner: "0xSeller",
      opts: { fetchImpl: brokenStub },
    });
    expect(res).toMatchObject({ listing: null, source: "unavailable" });
    expect(res.owner).toBe("0xseller");
  });

  it("says empty when the seller really has nothing listed", async () => {
    const res = await loadCancelListing({
      owner: "0xSeller",
      opts: { fetchImpl: jsonStub({ data: [], total: 0 }) },
    });
    expect(res).toMatchObject({ listing: null, source: "empty" });
  });
});

describe("loadPacks", () => {
  it("says unavailable rather than 'no packs' when the purchase catalogue fails to load", async () => {
    const res = await loadPacks({ fetchImpl: brokenStub });
    expect(res).toMatchObject({ data: [], source: "unavailable" });
    expect(res.reason).toBeTruthy();
  });

  it("says empty when the node genuinely sells no packs", async () => {
    const res = await loadPacks({ fetchImpl: jsonStub([]) });
    expect(res).toMatchObject({ data: [], source: "empty" });
  });

  it("says live when packs come back", async () => {
    const pack = {
      sku: "credits-100",
      title: "100 Credits",
      credits: "100",
      priceCents: 1000,
      currency: "usd",
      sortOrder: 1,
    };
    const res = await loadPacks({ fetchImpl: jsonStub([pack]) });
    expect(res.source).toBe("live");
    expect(res.data).toHaveLength(1);
  });
});

describe("loadStore", () => {
  it("says unavailable when the content server cannot be reached", async () => {
    vi.stubGlobal("fetch", throwingStub);
    const res = await loadStore("0xowner", { base: "http://catalyst.invalid" });
    expect(res.source).toBe("unavailable");
    expect(res.reason).toBeTruthy();
  });

  it("says unavailable on a non-ok answer", async () => {
    vi.stubGlobal("fetch", brokenStub);
    const res = await loadStore("0xowner", { base: "http://catalyst.invalid" });
    expect(res.source).toBe("unavailable");
  });

  it("says empty only when the address has published no store", async () => {
    vi.stubGlobal("fetch", jsonStub([]));
    const res = await loadStore("0xowner", { base: "http://catalyst.invalid" });
    expect(res.source).toBe("empty");
  });
});
