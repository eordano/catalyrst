import { describe, expect, it } from "vitest";
import { encodeFunctionData, hashTypedData, parseAbi } from "viem";
import { privateKeyToAccount } from "viem/accounts";

import {
  MANA_POLYGON,
  chainIdSalt,
  executeMetaTxCalldata,
  manaMetaTxTypedData,
  normalizeV,
  transferCalldata,
} from "./mana-pay";

const PAY_TO = "0x2a7fa4cad84d8ca921bd6c04f4462d99e35db6a2";
const FROM = "0x951bb66ce4a5d4b1c667e386af5313753d14ba2e";
const WEI = "1250000000000000000";

const ERC20 = parseAbi([
  "function transfer(address to, uint256 amount) returns (bool)",
]);
const EXECUTE = parseAbi([
  "function executeMetaTransaction(address userAddress, bytes functionSignature, bytes32 sigR, bytes32 sigS, uint8 sigV) payable returns (bytes)",
]);

describe("transferCalldata", () => {
  it("matches viem's independent ABI encoding", () => {
    const mine = transferCalldata(PAY_TO, WEI);
    const viems = encodeFunctionData({
      abi: ERC20,
      functionName: "transfer",
      args: [PAY_TO, BigInt(WEI)],
    });
    expect(mine).toBe(viems);
  });
});

describe("executeMetaTxCalldata", () => {
  it("matches viem's independent ABI encoding (classic path, 0xa0 offset)", async () => {
    const fn = transferCalldata(PAY_TO, WEI);
    const account = privateKeyToAccount(
      "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    );
    const typed = manaMetaTxTypedData(FROM, "7", fn);
    const types = { ...typed.types } as Record<
      string,
      Array<{ name: string; type: string }>
    >;
    delete types.EIP712Domain;
    const sig = await account.signTypedData({
      domain: {
        name: typed.domain.name,
        version: typed.domain.version,
        verifyingContract: typed.domain.verifyingContract as `0x${string}`,
        salt: typed.domain.salt as `0x${string}`,
      },
      types,
      primaryType: "MetaTransaction",
      message: { nonce: 7n, from: FROM, functionSignature: fn },
    });

    const r = `0x${sig.slice(2, 66)}` as `0x${string}`;
    const s = `0x${sig.slice(66, 130)}` as `0x${string}`;
    const v = parseInt(sig.slice(130, 132), 16);

    const mine = executeMetaTxCalldata(FROM, sig, fn);
    const viems = encodeFunctionData({
      abi: EXECUTE,
      functionName: "executeMetaTransaction",
      args: [FROM, fn as `0x${string}`, r, s, v < 27 ? v + 27 : v],
    });
    expect(mine).toBe(viems);
  });
});

describe("typed data", () => {
  it("salt encodes chain 137 as bytes32", () => {
    expect(chainIdSalt(137)).toBe(
      "0x0000000000000000000000000000000000000000000000000000000000000089",
    );
  });

  it("hashes without throwing under viem (structure is EIP-712 valid)", () => {
    const fn = transferCalldata(PAY_TO, WEI);
    const typed = manaMetaTxTypedData(FROM, "0", fn);
    const types = { ...typed.types } as Record<
      string,
      Array<{ name: string; type: string }>
    >;
    delete types.EIP712Domain;
    const digest = hashTypedData({
      domain: {
        name: typed.domain.name,
        version: typed.domain.version,
        verifyingContract: typed.domain.verifyingContract as `0x${string}`,
        salt: typed.domain.salt as `0x${string}`,
      },
      types,
      primaryType: "MetaTransaction",
      message: { nonce: 0n, from: FROM, functionSignature: fn },
    });
    expect(digest).toMatch(/^0x[0-9a-f]{64}$/);
  });

  it("pins the production MANA polygon domain", () => {
    expect(MANA_POLYGON.address).toBe(
      "0xa1c57f48f0deb89f569dfbe6e2b7f46d33606fd4",
    );
    expect(MANA_POLYGON.domainName).toBe("(PoS) Decentraland MANA");
    expect(MANA_POLYGON.domainVersion).toBe("1");
  });
});

describe("normalizeV", () => {
  it("lifts ledger-style 00/01 to 1b/1c and rejects garbage", () => {
    expect(normalizeV("00")).toBe("1b");
    expect(normalizeV("01")).toBe("1c");
    expect(normalizeV("1b")).toBe("1b");
    expect(normalizeV("1c")).toBe("1c");
    expect(() => normalizeV("05")).toThrow();
  });
});
