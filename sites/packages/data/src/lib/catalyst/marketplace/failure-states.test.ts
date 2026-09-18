import { afterEach, describe, expect, it, vi } from "vitest";

import { parseBuyOrder, fetchOrders, fetchCheapestOpenOrder, fetchOpenOrderForToken } from "./buy";
import { fetchReceivedBids } from "./bids";
import { loadReceivedBids } from "./bids.server";
import { loadCancelListing } from "./orders.server";
import { loadPacks } from "./packs.server";
import { loadStore } from "./settings.server";

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
  it("returns the order when it validates and null rather than casting an unvalidated row through to a purchase", () => {
    expect(parseBuyOrder(ORDER)?.price).toBe("1000000000000000000");
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
  it("reports empty, unavailable (throw or all-invalid), or the validated order, for the cheapest and per-token lookups alike", async () => {
    expect(
      await fetchCheapestOpenOrder("0xc", { fetchImpl: jsonStub({ data: [], total: 0 }) }),
    ).toMatchObject({ order: null, source: "empty" });

    const thrown = await fetchCheapestOpenOrder("0xc", { fetchImpl: throwingStub });
    expect(thrown.source).toBe("unavailable");
    expect(thrown.order).toBeNull();
    expect(thrown.reason).toBeTruthy();

    expect(
      (
        await fetchCheapestOpenOrder("0xc", {
          fetchImpl: jsonStub({ data: [{ id: "bad" }], total: 1 }),
        })
      ).source,
    ).toBe("unavailable");

    const live = await fetchCheapestOpenOrder("0xc", {
      fetchImpl: jsonStub({ data: [ORDER], total: 1 }),
    });
    expect(live.source).toBe("catalyst");
    expect(live.order?.id).toBe(ORDER.id);

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
  it("says unavailable when the read fails, the envelope carries no payload, or no returned bid validates", async () => {
    const broken = await fetchReceivedBids("0xseller", { fetchImpl: brokenStub });
    expect(broken).toMatchObject({ bids: [], source: "unavailable" });
    expect(broken.reason).toBeTruthy();

    expect(
      (await fetchReceivedBids("0xseller", { fetchImpl: jsonStub({ ok: false }) })).source,
    ).toBe("unavailable");

    expect(
      (
        await fetchReceivedBids("0xseller", {
          fetchImpl: jsonStub({ ok: true, data: { results: [{ nope: 1 }], total: 1 } }),
        })
      ).source,
    ).toBe("unavailable");
  });

  it("says empty only when the seller genuinely has no bids and live when there are bids", async () => {
    const empty = await fetchReceivedBids("0xseller", {
      fetchImpl: jsonStub({
        ok: true,
        data: { results: [], total: 0, page: 0, pages: 0, limit: 24 },
      }),
    });
    expect(empty).toMatchObject({ bids: [], source: "empty" });

    const live = await fetchReceivedBids("0xseller", {
      fetchImpl: jsonStub({
        ok: true,
        data: { results: [BID], total: 1, page: 0, pages: 1, limit: 24 },
      }),
    });
    expect(live.source).toBe("live");
    expect(live.bids).toHaveLength(1);
  });
});

describe("loadReceivedBids", () => {
  it("forwards the unavailable state to the accept-bid loader and is empty with no signed-in owner", async () => {
    expect((await loadReceivedBids("0xseller", { fetchImpl: brokenStub })).source).toBe(
      "unavailable",
    );
    expect((await loadReceivedBids(null)).source).toBe("empty");
  });
});

describe("loadCancelListing", () => {
  it("says unavailable rather than 'no active listing' when the read fails, and empty when the seller really has nothing listed", async () => {
    const broken = await loadCancelListing({
      owner: "0xSeller",
      opts: { fetchImpl: brokenStub },
    });
    expect(broken).toMatchObject({ listing: null, source: "unavailable" });
    expect(broken.owner).toBe("0xseller");

    const empty = await loadCancelListing({
      owner: "0xSeller",
      opts: { fetchImpl: jsonStub({ data: [], total: 0 }) },
    });
    expect(empty).toMatchObject({ listing: null, source: "empty" });
  });
});

describe("loadPacks", () => {
  it("says unavailable when the catalogue fails to load, empty when the node sells no packs, and live when packs come back", async () => {
    const broken = await loadPacks({ fetchImpl: brokenStub });
    expect(broken).toMatchObject({ data: [], source: "unavailable" });
    expect(broken.reason).toBeTruthy();

    expect(await loadPacks({ fetchImpl: jsonStub([]) })).toMatchObject({
      data: [],
      source: "empty",
    });

    const pack = {
      sku: "credits-100",
      title: "100 Credits",
      credits: "100",
      priceCents: 1000,
      currency: "usd",
      sortOrder: 1,
    };
    const live = await loadPacks({ fetchImpl: jsonStub([pack]) });
    expect(live.source).toBe("live");
    expect(live.data).toHaveLength(1);
  });
});

describe("loadStore", () => {
  it("says unavailable when the content server cannot be reached or answers non-ok, and empty only when no store is published", async () => {
    vi.stubGlobal("fetch", throwingStub);
    const unreachable = await loadStore("0xowner", { base: "http://catalyst.invalid" });
    expect(unreachable.source).toBe("unavailable");
    expect(unreachable.reason).toBeTruthy();

    vi.stubGlobal("fetch", brokenStub);
    expect((await loadStore("0xowner", { base: "http://catalyst.invalid" })).source).toBe(
      "unavailable",
    );

    vi.stubGlobal("fetch", jsonStub([]));
    expect((await loadStore("0xowner", { base: "http://catalyst.invalid" })).source).toBe(
      "empty",
    );
  });
});
