import { describe, expect, it, vi } from "vitest";

import {
  encodeDelegationQuery,
  encodeSetDelegate,
  GLOBAL_SPACE_ID,
  readDelegation,
  resolveDelegateRegistry,
  snapshotSpaceId,
  type DelegateRegistryConfig,
} from "./delegate-registry";

const REGISTRY: DelegateRegistryConfig = {
  address: "0x469788fe6e9e9681c6ebf3bf78e7fd26fc015446",
  chainId: 1,
};

const SPACE = "snapshot.dcl.eth";
const SPACE_ID =
  "0x736e617073686f742e64636c2e65746800000000000000000000000000000000";

function rpcOk(results: string[]): typeof fetch {
  const queue = [...results];
  return vi.fn(async () =>
    new Response(JSON.stringify({ jsonrpc: "2.0", id: 1, result: queue.shift() }), {
      status: 200,
      headers: { "content-type": "application/json" },
    }),
  ) as unknown as typeof fetch;
}

const NO_DELEGATE = `0x${"0".repeat(64)}`;
const DELEGATE_5985 =
  "0x0000000000000000000000005985eb4a8e0e1f7bca9cc0d7ae81c2943fb205bd";

describe("snapshot space id", () => {
  it("is the utf-8 space right-padded to bytes32 and rejects a space that does not fit", () => {
    expect(snapshotSpaceId(SPACE)).toBe(SPACE_ID);
    expect(() => snapshotSpaceId("x".repeat(33))).toThrow(/bytes32/);
  });
});

describe("delegate registry calldata", () => {
  it("encodes setDelegate (0xbd86e508) and delegation (0x74c6c454) with their selectors and refuses a non-address delegate", () => {
    expect(encodeSetDelegate(SPACE, "0xc1b8662A68F3eb66bC5e5C4DE7C1EF04Dc344d53")).toBe(
      "0xbd86e508" +
        "736e617073686f742e64636c2e65746800000000000000000000000000000000" +
        "000000000000000000000000c1b8662a68f3eb66bc5e5c4de7c1ef04dc344d53",
    );
    expect(
      encodeDelegationQuery(
        "0xc1b8662A68F3eb66bC5e5C4DE7C1EF04Dc344d53",
        snapshotSpaceId(SPACE),
      ),
    ).toBe(
      "0x74c6c454" +
        "000000000000000000000000c1b8662a68f3eb66bc5e5c4de7c1ef04dc344d53" +
        "736e617073686f742e64636c2e65746800000000000000000000000000000000",
    );
    expect(() => encodeSetDelegate(SPACE, "metahero.dcl")).toThrow(/ethereum address/);
  });
});

describe("readDelegation", () => {
  it("returns the space-scoped delegate from a real registry eth_call, falling back to the global space id when the space slot is empty", async () => {
    const fetchImpl = rpcOk([DELEGATE_5985]);
    const state = await readDelegation({
      registry: REGISTRY,
      rpcUrl: "https://rpc.example",
      space: SPACE,
      delegator: "0x247e0896706bb09245549e476257a0a1129db418",
      fetchImpl,
    });

    expect(state).toEqual({
      delegate: "0x5985eb4a8e0e1f7bca9cc0d7ae81c2943fb205bd",
      scope: "space",
    });
    const call = vi.mocked(fetchImpl).mock.calls[0];
    expect(call[0]).toBe("https://rpc.example");
    const body = JSON.parse(String(call[1]?.body));
    expect(body.method).toBe("eth_call");
    expect(body.params[0].to).toBe(REGISTRY.address);
    expect(body.params[1]).toBe("latest");

    const fallback = rpcOk([NO_DELEGATE, DELEGATE_5985]);
    const global = await readDelegation({
      registry: REGISTRY,
      rpcUrl: "https://rpc.example",
      space: SPACE,
      delegator: "0x247e0896706bb09245549e476257a0a1129db418",
      fetchImpl: fallback,
    });
    expect(global.scope).toBe("global");
    const second = JSON.parse(String(vi.mocked(fallback).mock.calls[1][1]?.body));
    expect(second.params[0].data.endsWith(GLOBAL_SPACE_ID.slice(2))).toBe(true);
  });

  it("reports no delegation when both slots are empty and throws on an RPC error instead of reporting none", async () => {
    const state = await readDelegation({
      registry: REGISTRY,
      rpcUrl: "https://rpc.example",
      space: SPACE,
      delegator: "0xc5f705ca8f70acc5f3d3db9636558102f528a475",
      fetchImpl: rpcOk([NO_DELEGATE, NO_DELEGATE]),
    });
    expect(state).toEqual({ delegate: null, scope: "none" });

    const failing = vi.fn(async () =>
      new Response(JSON.stringify({ error: { message: "execution reverted" } }), {
        status: 200,
      }),
    ) as unknown as typeof fetch;
    await expect(
      readDelegation({
        registry: REGISTRY,
        rpcUrl: "https://rpc.example",
        space: SPACE,
        delegator: "0xc5f705ca8f70acc5f3d3db9636558102f528a475",
        fetchImpl: failing,
      }),
    ).rejects.toThrow(/execution reverted/);
  });
});

describe("resolveDelegateRegistry", () => {
  it("fails closed with named blockers when the contract is unset or the chain id is not a number, and resolves a configured registry with none", () => {
    const unset = resolveDelegateRegistry({});
    expect(unset.config).toBeNull();
    expect(unset.rpcUrl).toBeNull();
    expect(unset.blockers[0]).toMatch(
      /SNAPSHOT_DELEGATE_CONTRACT_ADDRESS and SNAPSHOT_DELEGATE_CHAIN_ID/,
    );

    const badChain = resolveDelegateRegistry({
      SNAPSHOT_DELEGATE_CONTRACT_ADDRESS: REGISTRY.address,
      SNAPSHOT_DELEGATE_CHAIN_ID: "mainnet",
    });
    expect(badChain.config).toBeNull();
    expect(badChain.blockers.some((b) => /CHAIN_ID is not a chain id/.test(b))).toBe(true);

    expect(
      resolveDelegateRegistry({
        SNAPSHOT_DELEGATE_CONTRACT_ADDRESS: "0x469788fE6E9E9681C6ebF3bF78e7Fd26Fc015446",
        SNAPSHOT_DELEGATE_CHAIN_ID: "1",
        SNAPSHOT_DELEGATE_RPC_URL: "https://rpc.example",
      }),
    ).toEqual({
      config: REGISTRY,
      rpcUrl: "https://rpc.example",
      blockers: [],
    });
  });
});
