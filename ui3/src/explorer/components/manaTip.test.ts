import { describe, expect, it } from "vitest";

import {
  chainName,
  encodeManaTransfer,
  manaToWei,
  sendManaTip,
  tipTargetFromConfig,
} from "./manaTip";

const OWNER = "0x92de52247aeae00fcfb18072c8564f3549b64f9c";
const MANA_POLYGON = "0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4";

type Call = { method: string; params?: unknown[] };

function fakeProvider(chainId: string, hash: unknown = "0xhash") {
  const calls: Call[] = [];
  let current = chainId;
  const provider = {
    request: async ({ method, params }: { method: string; params?: unknown[] }) => {
      calls.push({ method, params });
      if (method === "eth_chainId") return current;
      if (method === "wallet_switchEthereumChain") {
        current = (params?.[0] as { chainId: string }).chainId;
        return null;
      }
      if (method === "eth_sendTransaction") return hash;
      throw new Error(`unexpected ${method}`);
    },
  };
  return { provider, calls };
}

describe("manaToWei", () => {
  it("parses whole and fractional MANA into wei and rejects zero, negatives and non-numbers", () => {
    expect(manaToWei("1")).toBe(1_000_000_000_000_000_000n);
    expect(manaToWei("1.5")).toBe(1_500_000_000_000_000_000n);
    expect(manaToWei(" 0.000001 ")).toBe(1_000_000_000_000n);
    expect(manaToWei("0")).toBeNull();
    expect(manaToWei("-3")).toBeNull();
    expect(manaToWei("abc")).toBeNull();
    expect(manaToWei("")).toBeNull();
    expect(manaToWei("1e3")).toBeNull();
  });
});

describe("encodeManaTransfer", () => {
  it("emits the ERC-20 transfer selector with the padded recipient and amount, and refuses a malformed recipient", () => {
    const wei = 5_000_000_000_000_000_000n;
    const data = encodeManaTransfer(OWNER, wei);
    expect(data.slice(0, 10)).toBe("0xa9059cbb");
    expect(data.slice(10, 74)).toBe("0".repeat(24) + OWNER.slice(2));
    expect(BigInt("0x" + data.slice(74))).toBe(wei);
    expect(data.length).toBe(2 + 8 + 64 + 64);
    expect(() => encodeManaTransfer("0x1234", 1n)).toThrow(/valid wallet address/);
  });
});

describe("tipTargetFromConfig and chainName", () => {
  it("accepts a configured chain and MANA token, refuses a missing or malformed config, and names the chains MANA lives on", () => {
    expect(
      tipTargetFromConfig({ chainId: 137, enabled: true, manaToken: MANA_POLYGON, payTo: null }),
    ).toEqual({ chainId: 137, manaToken: MANA_POLYGON });
    expect(tipTargetFromConfig({ chainId: 137, enabled: false, manaToken: null, payTo: null })).toBeNull();
    expect(tipTargetFromConfig(null)).toBeNull();
    expect(tipTargetFromConfig({ chainId: "137", manaToken: MANA_POLYGON })).toBeNull();
    expect(chainName(1)).toBe("Ethereum");
    expect(chainName(137)).toBe("Polygon");
    expect(chainName(4242)).toBe("chain 4242");
  });
});

describe("sendManaTip", () => {
  const target = { chainId: 137, manaToken: MANA_POLYGON };
  const from = "0x0000000000000000000000000000000000000001";

  it("sends straight away on the right chain, switches chains first otherwise, and fails loudly without a hash", async () => {
    const direct = fakeProvider("0x89");
    const hash = await sendManaTip(direct.provider, { from, to: OWNER, wei: 1n, target });
    expect(hash).toBe("0xhash");
    expect(direct.calls.map((c) => c.method)).toEqual(["eth_chainId", "eth_sendTransaction"]);
    const tx = direct.calls[1]?.params?.[0] as { from: string; to: string; data: string };
    expect(tx.from).toBe(from);
    expect(tx.to).toBe(MANA_POLYGON);
    expect(tx.data.startsWith("0xa9059cbb")).toBe(true);

    const elsewhere = fakeProvider("0x1");
    await sendManaTip(elsewhere.provider, { from, to: OWNER, wei: 1n, target });
    expect(elsewhere.calls.map((c) => c.method)).toEqual([
      "eth_chainId",
      "wallet_switchEthereumChain",
      "eth_sendTransaction",
    ]);
    expect(elsewhere.calls[1]?.params?.[0]).toEqual({ chainId: "0x89" });

    const { provider } = fakeProvider("0x89", null);
    await expect(sendManaTip(provider, { from, to: OWNER, wei: 1n, target })).rejects.toThrow(
      /no transaction hash/,
    );
  });
});
