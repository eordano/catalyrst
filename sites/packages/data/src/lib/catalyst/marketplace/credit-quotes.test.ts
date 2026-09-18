import { describe, expect, it } from "vitest";

import {
  CREDIT_USD,
  CREDIT_USD_DECIMAL,
  CREDITS_PER_USD,
} from "@ui/generated/catalyst/credits/CreditPeg";
import { PriceQuotesSchema, tryQuoteCreditItems, tryQuoteCreditPrices } from "./credit-quotes";

describe("Credits peg (generated from catalyrst-credits CREDIT_USD)", () => {
  it("is the strategy peg (1 Credit = 0.10 USDC, 10 Credits per USD) and its forms agree numerically", () => {
    expect(CREDIT_USD).toBe(0.1);
    expect(CREDIT_USD_DECIMAL).toBe("0.10");
    expect(CREDITS_PER_USD).toBe(10);
    expect(Number(CREDIT_USD_DECIMAL)).toBeCloseTo(CREDIT_USD, 12);
    expect(CREDITS_PER_USD * CREDIT_USD).toBeCloseTo(1, 12);
  });
});

describe("PriceQuotesSchema", () => {
  it("parses the service wire shape and rejects numbers where credits strings are expected", () => {
    const parsed = PriceQuotesSchema.parse({
      items: [
        { itemId: "0", collection: "0xabc", credits: "2" },
        { itemId: "1", collection: "0xabc", credits: null },
      ],
      amounts: ["1", null],
    });
    expect(parsed.items[0].credits).toBe("2");
    expect(parsed.amounts).toEqual(["1", null]);

    expect(() =>
      PriceQuotesSchema.parse({ items: [], amounts: [2] }),
    ).toThrow();
  });
});

describe("tryQuoteCreditPrices", () => {
  it("degrades to index-aligned nulls when the service is unreachable and short-circuits empty requests without fetching", async () => {
    const failingFetch = (async () => {
      throw new Error("down");
    }) as unknown as typeof fetch;
    const out = await tryQuoteCreditPrices(
      {
        items: [{ itemId: "7", collection: "0xdef" }],
        amounts: ["10000000000000000", null],
      },
      { fetchImpl: failingFetch },
    );
    expect(out.items).toEqual([{ itemId: "7", collection: "0xdef", credits: null }]);
    expect(out.amounts).toEqual([null, null]);

    let called = 0;
    const spyFetch = (async () => {
      called++;
      return new Response("{}");
    }) as unknown as typeof fetch;
    const empty = await tryQuoteCreditPrices({}, { fetchImpl: spyFetch });
    expect(called).toBe(0);
    expect(empty).toEqual({ items: [], amounts: [] });
  });
});

describe("tryQuoteCreditItems \u{2014} charge-basis item quoting", () => {
  const ref = (n: number) => ({
    itemId: String(n),
    collection: "0x59a90bad9570ecd08895f132daf7b79696337f61",
  });

  function fetchReturningCredits(
    creditsFor: (itemId: string) => string | null,
    calls: { bodies: unknown[] } = { bodies: [] },
  ): typeof fetch {
    return (async (_url: RequestInfo | URL, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body ?? "{}")) as {
        items: { itemId: string; collection: string }[];
      };
      calls.bodies.push(body);
      return new Response(
        JSON.stringify({
          items: body.items.map((r) => ({ ...r, credits: creditsFor(r.itemId) })),
          amounts: [],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }) as unknown as typeof fetch;
  }

  it("returns credits index-aligned with the refs (nulls preserved) and makes no request when every ref is null", async () => {
    const out = await tryQuoteCreditItems([ref(1), null, ref(3)], {
      fetchImpl: fetchReturningCredits((id) => (id === "3" ? null : "5")),
    });
    expect(out).toEqual(["5", null, null]);

    let called = 0;
    const spyFetch = (async () => {
      called++;
      return new Response("{}");
    }) as unknown as typeof fetch;
    const allNull = await tryQuoteCreditItems([null, null], { fetchImpl: spyFetch });
    expect(called).toBe(0);
    expect(allNull).toEqual([null, null]);
  });

  it("chunks batches above the server cap, stitches results back in order, and degrades a failing batch to nulls without failing the others", async () => {
    const calls = { bodies: [] as unknown[] };
    const refs = Array.from({ length: 61 }, (_, i) => ref(i));
    const out = await tryQuoteCreditItems(refs, {
      fetchImpl: fetchReturningCredits((id) => id, calls),
    });
    expect(calls.bodies).toHaveLength(2);
    expect(out).toHaveLength(61);
    expect(out[0]).toBe("0");
    expect(out[60]).toBe("60");

    let call = 0;
    const flaky = (async (_url: RequestInfo | URL, init?: RequestInit) => {
      call++;
      if (call === 1) throw new Error("down");
      const body = JSON.parse(String(init?.body ?? "{}")) as {
        items: { itemId: string; collection: string }[];
      };
      return new Response(
        JSON.stringify({
          items: body.items.map((r) => ({ ...r, credits: "9" })),
          amounts: [],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }) as unknown as typeof fetch;
    const partial = await tryQuoteCreditItems(refs, { fetchImpl: flaky });
    expect(partial.slice(0, 60).every((c) => c === null)).toBe(true);
    expect(partial[60]).toBe("9");
  });
});
