import assert from "node:assert";
import { afterEach, beforeEach, describe, it } from "node:test";
import * as OffChainMarketplaceABI from "../abi/DecentralandMarketplaceEthereum";
import type { BlockData, Context } from "./processor";
import {
  beginOffChainMarketplaceFeeBatch,
  endOffChainMarketplaceFeeBatch,
  getOffChainMarketplaceFeeRate,
  resetOffChainMarketplaceContractData,
  setOffChainMarketplaceFeeRate,
} from "./state";

const V2 = "0x1b67d0e31eeb6b52d8eeed71d3616c2f5b33b8e7";
const V3 = "0x0f11d0d1671519683bd48abf3dbe779e300941cd";
const N = 25900000;

const ctx = {} as unknown as Context;
const blockAt = (height: number) =>
  ({ header: { height } } as unknown as BlockData);

const contractReads: number[] = [];
const notStubbed = async (): Promise<bigint> => {
  throw new Error("no contract stub installed");
};
let readFeeRate: (height: number) => Promise<bigint> = notStubbed;

const realContract = (
  OffChainMarketplaceABI as unknown as { Contract: unknown }
).Contract;
const contractStub = function (_ctx: unknown, block: { height: number }) {
  contractReads.push(block.height);
  return { feeRate: () => readFeeRate(block.height) };
};

describe("getOffChainMarketplaceFeeRate", () => {
  beforeEach(() => {
    (OffChainMarketplaceABI as unknown as { Contract: unknown }).Contract =
      contractStub;
    resetOffChainMarketplaceContractData();
    contractReads.length = 0;
    readFeeRate = notStubbed;
  });

  afterEach(() => {
    (OffChainMarketplaceABI as unknown as { Contract: unknown }).Contract =
      realContract;
  });

  describe("when the cache is cold and a rate update lands later in the trade's block", () => {
    let feeRate: bigint;

    beforeEach(async () => {
      readFeeRate = async (height) =>
        height >= N ? BigInt(40000) : BigInt(25000);
      feeRate = await getOffChainMarketplaceFeeRate(ctx, blockAt(N), V3);
    });

    it("should read the block before the trade's, so the later update is not in the state it sees", () => {
      assert.deepStrictEqual(contractReads, [N - 1]);
    });

    it("should give the trade the rate in force before the block", () => {
      assert.strictEqual(feeRate, BigInt(25000));
    });
  });

  describe("when a batch holds a trade, a rate update and another trade, in that order", () => {
    let beforeUpdate: bigint;
    let afterUpdate: bigint;

    beforeEach(async () => {
      readFeeRate = async () => BigInt(25000);
      beginOffChainMarketplaceFeeBatch(N);
      beforeUpdate = await getOffChainMarketplaceFeeRate(ctx, blockAt(N), V3);
      setOffChainMarketplaceFeeRate(V3, BigInt(40000));
      afterUpdate = await getOffChainMarketplaceFeeRate(ctx, blockAt(N), V3);
    });

    it("should give the trade before the update the rate that was in force", () => {
      assert.strictEqual(beforeUpdate, BigInt(25000));
    });

    it("should give the trade after the update the new rate", () => {
      assert.strictEqual(afterUpdate, BigInt(40000));
    });
  });

  describe("when one marketplace's rate changes", () => {
    let other: bigint;

    beforeEach(async () => {
      readFeeRate = async () => BigInt(25000);
      beginOffChainMarketplaceFeeBatch(N);
      setOffChainMarketplaceFeeRate(V3, BigInt(40000));
      other = await getOffChainMarketplaceFeeRate(ctx, blockAt(N), V2);
    });

    it("should leave every other version on its own rate", () => {
      assert.strictEqual(other, BigInt(25000));
    });
  });

  describe("when a batch's writes are still open", () => {
    let afterRetry: bigint;

    beforeEach(async () => {
      readFeeRate = async () => BigInt(25000);
      beginOffChainMarketplaceFeeBatch(N);
      setOffChainMarketplaceFeeRate(V3, BigInt(40000));
      endOffChainMarketplaceFeeBatch(N + 100);
      beginOffChainMarketplaceFeeBatch(N);
      afterRetry = await getOffChainMarketplaceFeeRate(ctx, blockAt(N), V3);
    });

    it("should not let the retry read a rate the failed attempt learned", () => {
      assert.strictEqual(afterRetry, BigInt(25000));
    });
  });

  describe("when the cache is cold and the chain read fails", () => {
    it("should reject rather than hand back a rate it never read", async () => {
      readFeeRate = async () => {
        throw new Error("RPC unavailable");
      };

      await assert.rejects(
        () => getOffChainMarketplaceFeeRate(ctx, blockAt(N), V3),
        /RPC unavailable/
      );
    });
  });
});
