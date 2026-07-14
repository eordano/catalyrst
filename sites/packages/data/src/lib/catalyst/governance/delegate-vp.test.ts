import { beforeEach, describe, expect, it, vi } from "vitest";

import { clearVpCache } from "./snapshot-vp";
import { loadDelegateData } from "./delegate-vp";
import type { DelegateRegistrySetup } from "./delegate-registry";

beforeEach(() => {
  clearVpCache();
});

const SPACE = "snapshot.dcl.eth";
const USER = "0x247e0896706bb09245549e476257a0a1129db418";

const ENGAGEMENT = {
  voters: [
    { address: "0xc5f705ca8f70acc5f3d3db9636558102f528a475", votes: 25, vp: 0.06 },
    { address: "0x247e0896706bb09245549e476257a0a1129db418", votes: 19, vp: 9474166.08 },
    { address: "0x9dab43bc15bd29cfe3030ca1b7edb3eeb9f3b6c1", votes: 15, vp: 7958541.64 },
  ],
  weekly: [{ week_start: "2026-07-20", votes: 36 }],
};

const CONFIGURED: DelegateRegistrySetup = {
  config: { address: "0x469788fe6e9e9681c6ebf3bf78e7fd26fc015446", chainId: 1 },
  rpcUrl: "https://rpc.example",
  blockers: [],
};

const UNCONFIGURED: DelegateRegistrySetup = {
  config: null,
  rpcUrl: null,
  blockers: [
    "delegation is not configured: set SNAPSHOT_DELEGATE_CONTRACT_ADDRESS and SNAPSHOT_DELEGATE_CHAIN_ID",
    "current delegation is unknown: set SNAPSHOT_DELEGATE_RPC_URL to read the delegate registry",
  ],
};

const DELEGATE_WORD =
  "0x000000000000000000000000d6eff8f07caf3443a1178407d3de4129149d6ef6";

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

type Routes = {
  engagement?: () => Response;
  hub?: (variables: Record<string, string>) => Response;
  rpc?: (nth: number) => Response;
};

function router(routes: Routes) {
  let rpcCalls = 0;
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.includes("/votes/engagement")) {
      return routes.engagement?.() ?? json(ENGAGEMENT);
    }
    if (url.includes("/graphql")) {
      const body = JSON.parse(String(init?.body)) as {
        variables: Record<string, string>;
      };
      return routes.hub?.(body.variables) ?? json({ data: {} });
    }
    if (url.includes("rpc.example")) {
      return routes.rpc?.(rpcCalls++) ?? json({ result: `0x${"0".repeat(64)}` });
    }
    throw new Error(`unexpected fetch: ${url}`);
  }) as unknown as typeof fetch;
}

function vpFor(variables: Record<string, string>): Response {
  const data: Record<string, unknown> = {};
  Object.entries(variables)
    .filter(([key]) => key.startsWith("a"))
    .forEach(([key, address]) => {
      const index = key.slice(1);
      data[`v${index}`] = {
        vp: address === USER ? 9474166 : 1000,
        vp_by_strategy: address === USER ? [0, 9474166, 0, 0, 0, 0, 0, 0, 0] : [0, 0, 0, 1000, 0, 0, 0, 0, 0],
      };
    });
  return json({ data });
}

describe("loadDelegateData \u{2014} real reads only", () => {
  it("reads the roster, live voting power and the on-chain delegation", async () => {
    const fetchImpl = router({ hub: vpFor, rpc: () => json({ result: DELEGATE_WORD }) });

    const data = await loadDelegateData({
      address: USER,
      space: SPACE,
      base: "http://governance.example",
      hubUrl: "https://hub.example",
      setup: CONFIGURED,
      fetchImpl,
    });

    expect(data.source).toBe("live");
    expect(data.blockers).toEqual([]);
    expect(data.candidates.map((c) => c.address)).toEqual([
      "0xc5f705ca8f70acc5f3d3db9636558102f528a475",
      "0x9dab43bc15bd29cfe3030ca1b7edb3eeb9f3b6c1",
    ]);
    expect(data.candidates[0].vp).toBe(1000);
    expect(data.candidates[0].vpDistribution?.names).toBe(1000);
    expect(data.cards[0].activityLabel).toBe("25 votes in 180d");
    expect(data.userVp).toBe(9474166);
    expect(data.userVpLabel).toBe("9,474,166");
    expect(data.delegatedTo).toBe("0xd6eff8f07caf3443a1178407d3de4129149d6ef6");
    expect(data.delegationScope).toBe("space");
    expect(data.registry).toEqual(CONFIGURED.config);
  });

  it("never lists the signed-in wallet as its own delegate", async () => {
    const data = await loadDelegateData({
      address: USER,
      base: "http://governance.example",
      hubUrl: "https://hub.example",
      setup: CONFIGURED,
      fetchImpl: router({ hub: vpFor }),
    });
    expect(data.candidates.some((c) => c.address === USER)).toBe(false);
  });

  it("keeps the read half working while the write config is missing", async () => {
    const fetchImpl = router({ hub: vpFor });

    const data = await loadDelegateData({
      address: USER,
      base: "http://governance.example",
      hubUrl: "https://hub.example",
      setup: UNCONFIGURED,
      fetchImpl,
    });

    expect(data.registry).toBeNull();
    expect(data.candidates).toHaveLength(2);
    expect(data.delegatedTo).toBeNull();
    expect(data.delegationScope).toBe("unknown");
    expect(data.blockers).toEqual(UNCONFIGURED.blockers);
  });

  it("says so when the governance archive is down instead of inventing delegates", async () => {
    const data = await loadDelegateData({
      address: USER,
      base: "http://governance.example",
      hubUrl: "https://hub.example",
      setup: CONFIGURED,
      fetchImpl: router({ engagement: () => json({ error: "down" }, 503) }),
    });

    expect(data.source).toBe("error");
    expect(data.candidates).toEqual([]);
    expect(data.blockers[0]).toMatch(/delegate roster unavailable: governance engagement returned 503/);
  });

  it("says so when snapshot voting power is unavailable instead of showing zero", async () => {
    const data = await loadDelegateData({
      address: USER,
      base: "http://governance.example",
      hubUrl: "https://hub.example",
      setup: CONFIGURED,
      fetchImpl: router({ hub: () => json({ errors: [{ message: "rate limited" }] }) }),
    });

    expect(data.userVp).toBeNull();
    expect(data.userVpLabel).toBe("\u{2014}");
    expect(data.candidates[0].vp).toBeNull();
    expect(data.candidates[0].vpLabel).toBe("\u{2014}");
    expect(data.blockers.some((b) => /voting power unavailable: .*rate limited/.test(b))).toBe(true);
  });

  it("asks for a wallet before claiming any voting power", async () => {
    const fetchImpl = router({ hub: vpFor });

    const data = await loadDelegateData({
      base: "http://governance.example",
      hubUrl: "https://hub.example",
      setup: CONFIGURED,
      fetchImpl,
    });

    expect(data.needsWallet).toBe(true);
    expect(data.userVp).toBeNull();
    expect(data.delegationScope).toBe("unknown");
    const rpcCalls = vi
      .mocked(fetchImpl)
      .mock.calls.filter(([url]) => String(url).includes("rpc.example"));
    expect(rpcCalls).toHaveLength(0);
  });
});
