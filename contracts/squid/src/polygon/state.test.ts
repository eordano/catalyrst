import assert from "node:assert";
import { afterEach, beforeEach, describe, it, mock } from "node:test";
import * as OffChainMarketplaceABI from "./abi/DecentralandMarketplacePolygon";
import type { Block, Context } from "./processor";
import {
  beginOffChainMarketplaceFeeBatch,
  endOffChainMarketplaceFeeBatch,
  getOffChainMarketplaceContractData,
  resetOffChainMarketplaceContractData,
  setOffChainMarketplaceFeeCollector,
  setOffChainMarketplaceFeeRate,
  setOffChainMarketplaceRoyaltiesRate,
} from "./state";

const V1 = "0x540fb08edb56aae562864b390542c97f562825ba";
const V3 = "0xe38ef22abe871513555cba89adfe45ab4f548ada";
const COMMITTEE_MULTISIG = "0xb08e3e7cc815213304d884c88ca476ebc50eaab2";
const FEE_COLLECTOR_SAFE = "0x184e4d9a26add0af1eafc145550e890a421f16d7";
const N = 93600000;

const ctx = {} as unknown as Context;
const blockAt = (height: number) => ({ height } as unknown as Block);

type ChainFeeConfig = {
  feeCollector: string;
  feeRate: bigint;
  royaltiesRate: bigint;
};
type FeeConfig = Awaited<ReturnType<typeof getOffChainMarketplaceContractData>>;

const contractReads: number[] = [];
const notStubbed = (): ChainFeeConfig => {
  throw new Error("no contract stub installed");
};
let readContract: (height: number) => ChainFeeConfig = notStubbed;

const realContract = (
  OffChainMarketplaceABI as unknown as { Contract: unknown }
).Contract;
const contractStub = function (_ctx: unknown, block: { height: number }) {
  contractReads.push(block.height);
  return {
    feeCollector: async () => readContract(block.height).feeCollector,
    feeRate: async () => readContract(block.height).feeRate,
    royaltiesRate: async () => readContract(block.height).royaltiesRate,
  };
};

const seedThroughSetters = (address: string, feeCollector: string) => {
  setOffChainMarketplaceFeeCollector(address, feeCollector);
  setOffChainMarketplaceFeeRate(address, BigInt(25000));
  setOffChainMarketplaceRoyaltiesRate(address, BigInt(25000));
};

describe("getOffChainMarketplaceContractData", () => {
  beforeEach(() => {
    (OffChainMarketplaceABI as unknown as { Contract: unknown }).Contract =
      contractStub;
    resetOffChainMarketplaceContractData();
    contractReads.length = 0;
    readContract = notStubbed;
    mock.method(console, "log", () => undefined);
  });

  afterEach(() => {
    (OffChainMarketplaceABI as unknown as { Contract: unknown }).Contract =
      realContract;
    mock.restoreAll();
  });

  describe("when two marketplaces have been configured through the update setters", () => {
    beforeEach(() => {
      seedThroughSetters(V1, COMMITTEE_MULTISIG);
      seedThroughSetters(V3, FEE_COLLECTOR_SAFE);
    });

    it("should resolve each marketplace to its own fee collector", async () => {
      const [v1, v3] = await Promise.all([
        getOffChainMarketplaceContractData(ctx, blockAt(N), V1),
        getOffChainMarketplaceContractData(ctx, blockAt(N), V3),
      ]);

      assert.deepStrictEqual(
        [v1.feeCollector, v3.feeCollector],
        [COMMITTEE_MULTISIG, FEE_COLLECTOR_SAFE]
      );
    });

    describe("and the emitting address is spelled in a different case", () => {
      it("should resolve the same entry", async () => {
        const data = await getOffChainMarketplaceContractData(
          ctx,
          blockAt(N),
          V3.toUpperCase().replace("0X", "0x")
        );

        assert.strictEqual(data.feeCollector, FEE_COLLECTOR_SAFE);
      });
    });

    describe("and a fee update arrives from one of them", () => {
      beforeEach(() => {
        setOffChainMarketplaceFeeRate(V3, BigInt(40000));
      });

      it("should change only that marketplace's rate", async () => {
        const [v1, v3] = await Promise.all([
          getOffChainMarketplaceContractData(ctx, blockAt(N), V1),
          getOffChainMarketplaceContractData(ctx, blockAt(N), V3),
        ]);

        assert.deepStrictEqual(
          [v1.feeRate, v3.feeRate],
          [BigInt(25000), BigInt(40000)]
        );
      });
    });
  });

  describe("when the cache is cold and a fee update lands later in the trade's block", () => {
    let data: FeeConfig;

    beforeEach(async () => {
      readContract = (height) => ({
        feeCollector: FEE_COLLECTOR_SAFE,
        feeRate: height >= N ? BigInt(40000) : BigInt(25000),
        royaltiesRate: BigInt(25000),
      });
      data = await getOffChainMarketplaceContractData(ctx, blockAt(N), V3);
    });

    it("should read the block before the trade's, so the later update is not in the state it sees", () => {
      assert.deepStrictEqual(contractReads, [N - 1]);
    });

    it("should give the trade the rate in force before the block", () => {
      assert.strictEqual(data.feeRate, BigInt(25000));
    });
  });

  describe("when a batch is re-delivered starting inside the range it already covered", () => {
    let retry: FeeConfig;

    beforeEach(async () => {
      seedThroughSetters(V3, FEE_COLLECTOR_SAFE);
      readContract = () => ({
        feeCollector: FEE_COLLECTOR_SAFE,
        feeRate: BigInt(40000),
        royaltiesRate: BigInt(25000),
      });
      beginOffChainMarketplaceFeeBatch(100);
      setOffChainMarketplaceFeeRate(V3, BigInt(40000));
      endOffChainMarketplaceFeeBatch(200);
      beginOffChainMarketplaceFeeBatch(150);
      retry = await getOffChainMarketplaceContractData(ctx, blockAt(160), V3);
    });

    it("should read the chain again rather than keep a value the re-delivery cannot restore", () => {
      assert.deepStrictEqual(contractReads, [159]);
    });

    it("should give the trade the rate the chain reports", () => {
      assert.strictEqual(retry.feeRate, BigInt(40000));
    });
  });

  describe("when the cache is cold and the chain read fails", () => {
    it("should reject rather than hand back an empty configuration", async () => {
      readContract = () => {
        throw new Error("RPC unavailable");
      };

      await assert.rejects(
        () => getOffChainMarketplaceContractData(ctx, blockAt(N), V3),
        /RPC unavailable/
      );
    });
  });

  describe("when the cache is cold and an update earlier in the batch already set one field", () => {
    let data: FeeConfig;

    beforeEach(async () => {
      beginOffChainMarketplaceFeeBatch(N);
      setOffChainMarketplaceFeeRate(V3, BigInt(40000));
      readContract = () => ({
        feeCollector: FEE_COLLECTOR_SAFE,
        feeRate: BigInt(25000),
        royaltiesRate: BigInt(25000),
      });
      data = await getOffChainMarketplaceContractData(ctx, blockAt(N), V3);
    });

    it("should keep the replayed rate rather than overwrite it with the chain read", () => {
      assert.strictEqual(data.feeRate, BigInt(40000));
    });

    it("should still seed the fields nothing changed from the chain", () => {
      assert.strictEqual(data.feeCollector, FEE_COLLECTOR_SAFE);
    });
  });

  describe("when a batch with a trade before a fee update is retried after failing", () => {
    let firstAttempt: FeeConfig;
    let retry: FeeConfig;

    beforeEach(async () => {
      seedThroughSetters(V3, FEE_COLLECTOR_SAFE);
      beginOffChainMarketplaceFeeBatch(100);
      firstAttempt = await getOffChainMarketplaceContractData(
        ctx,
        blockAt(100),
        V3
      );
      setOffChainMarketplaceFeeRate(V3, BigInt(40000));
      endOffChainMarketplaceFeeBatch(200);
      beginOffChainMarketplaceFeeBatch(100);
      retry = await getOffChainMarketplaceContractData(ctx, blockAt(100), V3);
    });

    it("should give the trade the same rate on the retry as on the first attempt", () => {
      assert.deepStrictEqual(
        [firstAttempt.feeRate, retry.feeRate],
        [BigInt(25000), BigInt(25000)]
      );
    });
  });

  describe("when a batch stages a fee update and the next batch starts past it", () => {
    let data: FeeConfig;

    beforeEach(async () => {
      seedThroughSetters(V3, FEE_COLLECTOR_SAFE);
      beginOffChainMarketplaceFeeBatch(100);
      setOffChainMarketplaceFeeRate(V3, BigInt(40000));
      endOffChainMarketplaceFeeBatch(200);
      beginOffChainMarketplaceFeeBatch(201);
      data = await getOffChainMarketplaceContractData(ctx, blockAt(201), V3);
    });

    it("should promote the value, since the processor only moves past a batch it committed", () => {
      assert.strictEqual(data.feeRate, BigInt(40000));
    });
  });

  describe("when a batch throws before ending and the same range is retried", () => {
    let data: FeeConfig;

    beforeEach(async () => {
      seedThroughSetters(V3, FEE_COLLECTOR_SAFE);
      beginOffChainMarketplaceFeeBatch(100);
      setOffChainMarketplaceFeeRate(V3, BigInt(40000));
      beginOffChainMarketplaceFeeBatch(100);
      data = await getOffChainMarketplaceContractData(ctx, blockAt(100), V3);
    });

    it("should not let the retry see the failed batch's value", () => {
      assert.strictEqual(data.feeRate, BigInt(25000));
    });
  });

  describe("when a batch starts at or before a block whose writes were already promoted", () => {
    let data: FeeConfig;

    beforeEach(async () => {
      seedThroughSetters(V3, FEE_COLLECTOR_SAFE);
      beginOffChainMarketplaceFeeBatch(100);
      setOffChainMarketplaceFeeRate(V3, BigInt(40000));
      endOffChainMarketplaceFeeBatch(200);
      beginOffChainMarketplaceFeeBatch(201);
      beginOffChainMarketplaceFeeBatch(150);
      readContract = () => ({
        feeCollector: FEE_COLLECTOR_SAFE,
        feeRate: BigInt(25000),
        royaltiesRate: BigInt(25000),
      });
      data = await getOffChainMarketplaceContractData(ctx, blockAt(150), V3);
    });

    it("should drop the cache and re-seed from the chain rather than trust orphaned values", () => {
      assert.deepStrictEqual(
        [data.feeRate, contractReads.length],
        [BigInt(25000), 1]
      );
    });
  });
});
