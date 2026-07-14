import { describe, expect, it, vi } from "vitest";

import type { Eip1193Provider } from "../../auth/wallet";
import { buildDelegateVp, DelegationUnavailableError } from "./delegate-wallet";
import type { DelegateRegistryConfig } from "./delegate-registry";

const REGISTRY: DelegateRegistryConfig = {
  address: "0x469788fe6e9e9681c6ebf3bf78e7fd26fc015446",
  chainId: 1,
};

const SPACE = "snapshot.dcl.eth";
const WALLET = "0x247e0896706bb09245549e476257a0a1129db418";
const DELEGATE = "0x5985EB4a8e0e1f7BCa9Cc0D7AE81C2943fb205bd";
const TX_HASH = `0x${"ab".repeat(32)}`;

const EXPECTED_DATA =
  "0xbd86e508" +
  "736e617073686f742e64636c2e65746800000000000000000000000000000000" +
  "0000000000000000000000005985eb4a8e0e1f7bca9cc0d7ae81c2943fb205bd";

type Handler = (params: unknown[] | undefined, nth: number) => unknown;

function mockProvider(handlers: Record<string, Handler>) {
  const calls: { method: string; params?: unknown[] }[] = [];
  const counts = new Map<string, number>();
  const provider: Eip1193Provider = {
    async request({ method, params }) {
      calls.push({ method, params });
      const nth = counts.get(method) ?? 0;
      counts.set(method, nth + 1);
      const handler = handlers[method];
      if (!handler) throw new Error(`unexpected wallet call: ${method}`);
      return handler(params, nth);
    },
  };
  return { provider, calls, methods: () => calls.map((c) => c.method) };
}

const receipt = (status: string, block: string) => ({ status, blockNumber: block });

describe("buildDelegateVp \u{2014} the user-wallet setDelegate path", () => {
  it("sends the registry transaction the user signed for and returns the real receipt", async () => {
    const { provider, calls, methods } = mockProvider({
      eth_accounts: () => [WALLET],
      eth_chainId: () => "0x1",
      eth_sendTransaction: () => TX_HASH,
      eth_getTransactionReceipt: () => receipt("0x1", "0x1518b2"),
    });

    const delegate = buildDelegateVp({ registry: REGISTRY, provider });
    const out = await delegate({ space: SPACE, delegate: DELEGATE, vp: 12480 });

    expect(out).toEqual({
      space: SPACE,
      delegate: DELEGATE.toLowerCase(),
      vp: 12480,
      txHash: TX_HASH,
      chainId: 1,
      status: "confirmed",
      blockNumber: 0x1518b2,
    });

    const sent = calls.find((c) => c.method === "eth_sendTransaction")?.params?.[0];
    expect(sent).toEqual({
      from: WALLET,
      to: REGISTRY.address,
      data: EXPECTED_DATA,
    });
    expect(methods()).not.toContain("wallet_switchEthereumChain");
    expect(methods()).not.toContain("personal_sign");
  });

  it("switches the wallet to the registry chain before sending", async () => {
    const { provider, calls } = mockProvider({
      eth_accounts: () => [WALLET],
      eth_chainId: (_params, nth) => (nth === 0 ? "0x89" : "0x1"),
      wallet_switchEthereumChain: () => null,
      eth_sendTransaction: () => TX_HASH,
      eth_getTransactionReceipt: () => receipt("0x1", "0x10"),
    });

    const delegate = buildDelegateVp({ registry: REGISTRY, provider });
    await delegate({ space: SPACE, delegate: DELEGATE, vp: null });

    const switchCall = calls.find((c) => c.method === "wallet_switchEthereumChain");
    expect(switchCall?.params?.[0]).toEqual({ chainId: "0x1" });
  });

  it("never sends when the wallet stays on the wrong chain", async () => {
    const { provider, methods } = mockProvider({
      eth_accounts: () => [WALLET],
      eth_chainId: () => "0x89",
      wallet_switchEthereumChain: () => null,
      eth_sendTransaction: () => TX_HASH,
      eth_getTransactionReceipt: () => null,
    });

    const delegate = buildDelegateVp({ registry: REGISTRY, provider });
    await expect(delegate({ space: SPACE, delegate: DELEGATE, vp: null })).rejects.toThrow(
      /Wrong network: switch your wallet to chain 1/,
    );
    expect(methods()).not.toContain("eth_sendTransaction");
  });

  it("reports a rejected network switch without sending", async () => {
    const { provider, methods } = mockProvider({
      eth_accounts: () => [WALLET],
      eth_chainId: () => "0x89",
      wallet_switchEthereumChain: () => {
        throw Object.assign(new Error("User rejected the request."), { code: 4001 });
      },
      eth_sendTransaction: () => TX_HASH,
    });

    const delegate = buildDelegateVp({ registry: REGISTRY, provider });
    await expect(delegate({ space: SPACE, delegate: DELEGATE, vp: null })).rejects.toThrow(
      /Network switch rejected/,
    );
    expect(methods()).not.toContain("eth_sendTransaction");
  });

  it("fails closed with no registry configured and touches no wallet", async () => {
    const { provider, calls } = mockProvider({});

    const delegate = buildDelegateVp({ registry: null, provider });
    await expect(
      delegate({ space: SPACE, delegate: DELEGATE, vp: 1 }),
    ).rejects.toBeInstanceOf(DelegationUnavailableError);
    await expect(delegate({ space: SPACE, delegate: DELEGATE, vp: 1 })).rejects.toThrow(
      /delegate registry contract and chain are not configured/,
    );
    expect(calls).toEqual([]);
  });

  it("fails closed with no wallet in the page", async () => {
    const delegate = buildDelegateVp({ registry: REGISTRY });
    await expect(delegate({ space: SPACE, delegate: DELEGATE, vp: 1 })).rejects.toThrow(
      /No browser wallet found/,
    );
  });

  it("refuses self-delegation and a delegation that is already in place", async () => {
    const base = {
      eth_accounts: () => [WALLET],
      eth_chainId: () => "0x1",
      eth_sendTransaction: () => TX_HASH,
      eth_getTransactionReceipt: () => receipt("0x1", "0x1"),
    };

    const self = mockProvider(base);
    await expect(
      buildDelegateVp({ registry: REGISTRY, provider: self.provider })({
        space: SPACE,
        delegate: WALLET,
        vp: 1,
      }),
    ).rejects.toThrow(/cannot delegate to your own wallet/);
    expect(self.methods()).not.toContain("eth_sendTransaction");

    const repeat = mockProvider(base);
    await expect(
      buildDelegateVp({
        registry: REGISTRY,
        provider: repeat.provider,
        currentDelegate: DELEGATE,
      })({ space: SPACE, delegate: DELEGATE, vp: 1 }),
    ).rejects.toThrow(/already delegated to this address/);
    expect(repeat.methods()).not.toContain("eth_sendTransaction");
  });

  it("throws on a reverted receipt instead of reporting success", async () => {
    const { provider } = mockProvider({
      eth_accounts: () => [WALLET],
      eth_chainId: () => "0x1",
      eth_sendTransaction: () => TX_HASH,
      eth_getTransactionReceipt: () => receipt("0x0", "0x10"),
    });

    await expect(
      buildDelegateVp({ registry: REGISTRY, provider })({
        space: SPACE,
        delegate: DELEGATE,
        vp: 1,
      }),
    ).rejects.toThrow(/reverted on chain/);
  });

  it("reports pending \u{2014} not confirmed \u{2014} while the receipt is still missing", async () => {
    const { provider, calls } = mockProvider({
      eth_accounts: () => [WALLET],
      eth_chainId: () => "0x1",
      eth_sendTransaction: () => TX_HASH,
      eth_getTransactionReceipt: () => null,
    });

    const out = await buildDelegateVp({
      registry: REGISTRY,
      provider,
      pollIntervalMs: 1,
      confirmTimeoutMs: 5,
    })({ space: SPACE, delegate: DELEGATE, vp: 1 });

    expect(out.status).toBe("pending");
    expect(out.txHash).toBe(TX_HASH);
    expect(out.blockNumber).toBeNull();
    expect(calls.filter((c) => c.method === "eth_getTransactionReceipt").length).toBeGreaterThan(0);
  });

  it("prompts for accounts only when the wallet is locked", async () => {
    const { provider, methods } = mockProvider({
      eth_accounts: () => [],
      eth_requestAccounts: () => [WALLET],
      eth_chainId: () => "0x1",
      eth_sendTransaction: () => TX_HASH,
      eth_getTransactionReceipt: () => receipt("0x1", "0x2"),
    });

    await buildDelegateVp({ registry: REGISTRY, provider })({
      space: SPACE,
      delegate: DELEGATE,
      vp: null,
    });
    expect(methods()).toContain("eth_requestAccounts");
  });

  it("rejects a wallet that answers with something other than a tx hash", async () => {
    const { provider } = mockProvider({
      eth_accounts: () => [WALLET],
      eth_chainId: () => "0x1",
      eth_sendTransaction: () => "ok",
    });

    await expect(
      buildDelegateVp({ registry: REGISTRY, provider })({
        space: SPACE,
        delegate: DELEGATE,
        vp: 1,
      }),
    ).rejects.toThrow(/invalid transaction hash/);
  });
});

describe("no broadcast happens without an explicit provider call", () => {
  it("builds calldata without any network access", () => {
    const spy = vi.fn();
    const original = globalThis.fetch;
    globalThis.fetch = spy as unknown as typeof fetch;
    try {
      const { provider } = mockProvider({});
      expect(buildDelegateVp({ registry: REGISTRY, provider })).toBeTypeOf("function");
    } finally {
      globalThis.fetch = original;
    }
    expect(spy).not.toHaveBeenCalled();
  });
});
