import { beforeEach, describe, expect, it, vi } from "vitest";

import { clearVpCache, fetchVpDistributions, toVpDistribution } from "./snapshot-vp";

beforeEach(() => {
  clearVpCache();
});

const VOTER = "0xc1b8662a68f3eb66bc5e5c4de7c1ef04dc344d53";
const OTHER = "0x0000000000000000000000000000000000000001";

const HUB_RESPONSE = {
  data: {
    v0: {
      vp: 12195.939625551151,
      vp_by_strategy: [0, 2000, 8000, 600, 1.1234242518934, 5, 0, 100.74277568, 1489.073425619257],
    },
    v1: { vp: 100, vp_by_strategy: [0, 0, 0, 100, 0, 0, 0, 0, 0] },
  },
};

function hubOk(body: unknown): typeof fetch {
  return vi.fn(async () =>
    new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    }),
  ) as unknown as typeof fetch;
}

describe("toVpDistribution", () => {
  it("maps the snapshot strategy order onto the DAO breakdown", () => {
    const vp = toVpDistribution(12195.939625551151, [
      0, 2000, 8000, 600, 1.1234242518934, 5, 0, 100.74277568, 1489.073425619257,
    ]);
    expect(vp.land).toBe(2000);
    expect(vp.estate).toBe(8000);
    expect(vp.names).toBe(600);
    expect(vp.delegated).toBeCloseTo(1.1234242518934, 9);
    expect(vp.l1Wearables).toBe(5);
    expect(vp.rental).toBe(0);
    expect(vp.wMana).toBe(0);
    expect(vp.mana).toBeCloseTo(100.74277568 + 1489.073425619257, 9);
    expect(vp.own).toBeCloseTo(12195.939625551151 - 1.1234242518934, 9);
  });

  it("treats missing strategy slots as zero, never as unknown-but-positive", () => {
    const vp = toVpDistribution(0, []);
    expect(vp).toMatchObject({ total: 0, own: 0, delegated: 0, mana: 0, land: 0 });
  });
});

describe("fetchVpDistributions", () => {
  it("asks the hub for every address in one aliased query", async () => {
    const fetchImpl = hubOk(HUB_RESPONSE);
    const out = await fetchVpDistributions({
      space: "snapshot.dcl.eth",
      addresses: [VOTER, OTHER],
      hubUrl: "https://hub.example",
      fetchImpl,
    });

    expect(vi.mocked(fetchImpl)).toHaveBeenCalledTimes(1);
    const [url, init] = vi.mocked(fetchImpl).mock.calls[0];
    expect(url).toBe("https://hub.example/graphql");
    const body = JSON.parse(String(init?.body));
    expect(body.query).toContain("v0: vp(voter: $a0, space: $space)");
    expect(body.query).toContain("v1: vp(voter: $a1, space: $space)");
    expect(body.variables).toEqual({
      space: "snapshot.dcl.eth",
      a0: VOTER,
      a1: OTHER,
    });

    expect(out.get(VOTER)?.total).toBeCloseTo(12195.939625551151, 9);
    expect(out.get(OTHER)?.names).toBe(100);
  });

  it("chunks past the hub's five-root-selection cap", async () => {
    const addresses = Array.from(
      { length: 7 },
      (_, i) => `0x${String(i).repeat(40)}`.slice(0, 42),
    );
    const fetchImpl = vi.fn(async (_url: RequestInfo | URL, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body)) as { variables: Record<string, string> };
      const data: Record<string, unknown> = {};
      Object.keys(body.variables)
        .filter((k) => k.startsWith("a"))
        .forEach((k) => {
          data[`v${k.slice(1)}`] = { vp: 1, vp_by_strategy: [] };
        });
      return new Response(JSON.stringify({ data }), { status: 200 });
    }) as unknown as typeof fetch;

    const out = await fetchVpDistributions({
      space: "snapshot.dcl.eth",
      addresses,
      hubUrl: "https://hub.example",
      fetchImpl,
    });

    expect(vi.mocked(fetchImpl)).toHaveBeenCalledTimes(2);
    expect(out.size).toBe(7);
  });

  it("surfaces the hub's own message when it answers 500", async () => {
    const fetchImpl = vi.fn(async () =>
      new Response(
        JSON.stringify({
          errors: [{ message: "'vpBatch exceeds the maximum number of root selections: 5" }],
        }),
        { status: 500 },
      ),
    ) as unknown as typeof fetch;

    await expect(
      fetchVpDistributions({
        space: "snapshot.dcl.eth",
        addresses: [VOTER],
        hubUrl: "https://hub.example",
        fetchImpl,
      }),
    ).rejects.toThrow(/maximum number of root selections/);
  });

  it("returns no entry for an address the hub has no answer for", async () => {
    const out = await fetchVpDistributions({
      space: "snapshot.dcl.eth",
      addresses: [VOTER],
      hubUrl: "https://hub.example",
      fetchImpl: hubOk({ data: { v0: null } }),
    });
    expect(out.size).toBe(0);
  });

  it("throws when the hub reports an error rather than defaulting to zero", async () => {
    await expect(
      fetchVpDistributions({
        space: "snapshot.dcl.eth",
        addresses: [VOTER],
        hubUrl: "https://hub.example",
        fetchImpl: hubOk({ errors: [{ message: "rate limited" }] }),
      }),
    ).rejects.toThrow(/rate limited/);
  });

  it("reuses a fresh read and re-reads once the ttl lapses", async () => {
    const fetchImpl = hubOk(HUB_RESPONSE);
    const args = {
      space: "snapshot.dcl.eth",
      addresses: [VOTER],
      hubUrl: "https://hub.example",
      fetchImpl,
    };

    await fetchVpDistributions({ ...args, now: 1_000 });
    await fetchVpDistributions({ ...args, now: 1_000 + 60_000 });
    expect(vi.mocked(fetchImpl)).toHaveBeenCalledTimes(1);

    await fetchVpDistributions({ ...args, now: 1_000 + 6 * 60_000 });
    expect(vi.mocked(fetchImpl)).toHaveBeenCalledTimes(2);
  });

  it("makes no request when there is nothing to score", async () => {
    const fetchImpl = hubOk(HUB_RESPONSE);
    const out = await fetchVpDistributions({
      space: "snapshot.dcl.eth",
      addresses: ["not-an-address"],
      fetchImpl,
    });
    expect(out.size).toBe(0);
    expect(vi.mocked(fetchImpl)).not.toHaveBeenCalled();
  });
});
