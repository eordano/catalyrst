import { describe, expect, it } from "vitest";

import { fetchManaBalance } from "./topup";

const WALLET = "0x951BB66CE4A5D4B1C667E386AF5313753D14BA2E";

function jsonFetch(body: unknown, seen: string[]): typeof fetch {
  return (async (input: RequestInfo | URL) => {
    seen.push(String(input));
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  }) as typeof fetch;
}

describe("fetchManaBalance", () => {
  it("asks the economy service for the wallet's balance and returns wei as a bigint", async () => {
    const seen: string[] = [];
    const balance = await fetchManaBalance(WALLET, {
      fetchImpl: jsonFetch({ balance: "39580378408756389366" }, seen),
    });
    expect(balance).toBe(39580378408756389366n);
    expect(seen).toHaveLength(1);
    expect(seen[0]).toMatch(/\/v1\/payments\/balance\/0x951bb66ce4a5d4b1c667e386af5313753d14ba2e$/);
  });

  it("rejects a body that is not the generated wire shape", async () => {
    await expect(
      fetchManaBalance(WALLET, { fetchImpl: jsonFetch({ balance: 12 }, []) }),
    ).rejects.toThrow();
  });

  it("gives up on a hanging server instead of blocking the pane", async () => {
    const hanging = ((_: RequestInfo | URL, init?: RequestInit) =>
      new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => reject(init.signal?.reason));
      })) as typeof fetch;
    const started = Date.now();
    await expect(
      fetchManaBalance(WALLET, { fetchImpl: hanging, timeoutMs: 30 }),
    ).rejects.toThrow();
    expect(Date.now() - started).toBeLessThan(2000);
  });

  it("honours the caller's own abort signal alongside the timeout", async () => {
    const controller = new AbortController();
    const hanging = ((_: RequestInfo | URL, init?: RequestInit) =>
      new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => reject(init.signal?.reason));
      })) as typeof fetch;
    const pending = fetchManaBalance(WALLET, {
      fetchImpl: hanging,
      signal: controller.signal,
      timeoutMs: 60_000,
    });
    controller.abort();
    await expect(pending).rejects.toThrow();
  });
});
