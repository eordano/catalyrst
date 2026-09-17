import assert from "node:assert";
import { afterEach, beforeEach, describe, it, mock } from "node:test";
import * as OffChainMarketplaceABI from "../abi/DecentralandMarketplacePolygon";
import type { Block, BlockData, Context } from "../processor";
import {
  getOffChainMarketplaceContractData,
  resetOffChainMarketplaceContractData,
  setOffChainMarketplaceFeeCollector,
  setOffChainMarketplaceFeeRate,
  setOffChainMarketplaceRoyaltiesRate,
} from "../state";
import {
  applyFeeUpdate,
  FeeUpdateEventArgs,
  queueFeeUpdate,
  QueuedFeeUpdate,
} from "./feeUpdates";

const V2 = "0xa40b1d129b8906888720686f3a01921ddf37716f";
const V3 = "0xe38ef22abe871513555cba89adfe45ab4f548ada";
const CALLER = "0x0e659a116e161d8e502f9036babda51334f2667e";
const COMMITTEE_MULTISIG = "0xb08e3e7cc815213304d884c88ca476ebc50eaab2";
const FEE_COLLECTOR_SAFE = "0x184e4d9a26add0af1eafc145550e890a421f16d7";
const { FeeCollectorUpdated, FeeRateUpdated, RoyaltiesRateUpdated, Traded } =
  OffChainMarketplaceABI.events;

const pad = (hex: string) => "0x" + hex.replace(/^0x/, "").padStart(64, "0");
const word = (n: bigint) => n.toString(16).padStart(64, "0");

const feeLog = (
  address: string,
  topics: string[],
  data: string,
  logIndex: number
) =>
  ({
    address,
    logIndex,
    transactionHash: "0x" + "ab".repeat(32),
    topics,
    data,
  } as unknown as QueuedFeeUpdate["log"]);

const feeRateUpdatedLog = (address: string, rate: bigint, logIndex: number) =>
  feeLog(
    address,
    [FeeRateUpdated.topic, pad(CALLER)],
    "0x" + word(rate),
    logIndex
  );

const royaltiesRateUpdatedLog = (
  address: string,
  rate: bigint,
  logIndex: number
) =>
  feeLog(
    address,
    [RoyaltiesRateUpdated.topic, pad(CALLER)],
    "0x" + word(rate),
    logIndex
  );

const feeCollectorUpdatedLog = (
  address: string,
  collector: string,
  logIndex: number
) =>
  feeLog(
    address,
    [FeeCollectorUpdated.topic, pad(CALLER), pad(collector)],
    "0x",
    logIndex
  );

const ctx = {} as unknown as Context;
const block = { header: { height: 93600000 } } as unknown as BlockData;
const header = block.header as unknown as Block;

type FeeConfig = Awaited<ReturnType<typeof getOffChainMarketplaceContractData>>;

const seedThroughSetters = (address: string, feeCollector: string) => {
  setOffChainMarketplaceFeeCollector(address, feeCollector);
  setOffChainMarketplaceFeeRate(address, BigInt(25000));
  setOffChainMarketplaceRoyaltiesRate(address, BigInt(25000));
};

describe("applyFeeUpdate", () => {
  beforeEach(() => {
    resetOffChainMarketplaceContractData();
    seedThroughSetters(V2, COMMITTEE_MULTISIG);
    seedThroughSetters(V3, FEE_COLLECTOR_SAFE);
    mock.method(console, "log", () => undefined);
  });

  afterEach(() => {
    mock.restoreAll();
  });

  describe("when a batch holds a trade, a fee-rate update and another trade on one marketplace, in that order", () => {
    let beforeUpdate: FeeConfig;
    let afterUpdate: FeeConfig;
    let otherMarketplace: FeeConfig;

    beforeEach(async () => {
      beforeUpdate = await getOffChainMarketplaceContractData(ctx, header, V3);
      const queued = queueFeeUpdate(
        FeeRateUpdated.topic,
        feeRateUpdatedLog(V3, BigInt(40000), 2),
        block
      );
      assert.ok(queued);
      applyFeeUpdate(queued.topic, queued.log, queued.event);
      afterUpdate = await getOffChainMarketplaceContractData(ctx, header, V3);
      otherMarketplace = await getOffChainMarketplaceContractData(
        ctx,
        header,
        V2
      );
    });

    it("should give the trade before the update the rate that was in force", () => {
      assert.strictEqual(beforeUpdate.feeRate, BigInt(25000));
    });

    it("should give the trade after the update the new rate", () => {
      assert.strictEqual(afterUpdate.feeRate, BigInt(40000));
    });

    it("should leave the other marketplace's rate untouched", () => {
      assert.strictEqual(otherMarketplace.feeRate, BigInt(25000));
    });
  });

  describe("and the update is emitted by the older marketplace instead", () => {
    let v2: FeeConfig;
    let v3: FeeConfig;

    beforeEach(async () => {
      const queued = queueFeeUpdate(
        FeeRateUpdated.topic,
        feeRateUpdatedLog(V2, BigInt(40000), 2),
        block
      );
      assert.ok(queued);
      applyFeeUpdate(queued.topic, queued.log, queued.event);
      v2 = await getOffChainMarketplaceContractData(ctx, header, V2);
      v3 = await getOffChainMarketplaceContractData(ctx, header, V3);
    });

    it("should move the emitting marketplace", () => {
      assert.strictEqual(v2.feeRate, BigInt(40000));
    });

    it("should leave the newest marketplace on its own rate", () => {
      assert.strictEqual(v3.feeRate, BigInt(25000));
    });
  });

  describe("when a fee-collector update is applied", () => {
    let v3: FeeConfig;
    let v2: FeeConfig;

    beforeEach(async () => {
      const queued = queueFeeUpdate(
        FeeCollectorUpdated.topic,
        feeCollectorUpdatedLog(V3, COMMITTEE_MULTISIG, 5),
        block
      );
      assert.ok(queued);
      applyFeeUpdate(queued.topic, queued.log, queued.event);
      v3 = await getOffChainMarketplaceContractData(ctx, header, V3);
      v2 = await getOffChainMarketplaceContractData(ctx, header, V2);
    });

    it("should move that marketplace's collector and nothing else", () => {
      assert.deepStrictEqual(
        [v3.feeCollector.toLowerCase(), v3.feeRate, v2.feeCollector],
        [COMMITTEE_MULTISIG, BigInt(25000), COMMITTEE_MULTISIG]
      );
    });
  });

  describe("when a royalties-rate update is applied", () => {
    let v3: FeeConfig;

    beforeEach(async () => {
      const queued = queueFeeUpdate(
        RoyaltiesRateUpdated.topic,
        royaltiesRateUpdatedLog(V3, BigInt(10000), 6),
        block
      );
      assert.ok(queued);
      applyFeeUpdate(queued.topic, queued.log, queued.event);
      v3 = await getOffChainMarketplaceContractData(ctx, header, V3);
    });

    it("should change the royalties rate and leave the fee rate alone", () => {
      assert.deepStrictEqual(
        [v3.royaltiesRate, v3.feeRate],
        [BigInt(10000), BigInt(25000)]
      );
    });
  });

  describe("when the topic is not a fee update", () => {
    it("should report it as not handled and change nothing", () => {
      const handled = applyFeeUpdate(
        Traded.topic,
        { address: V3 },
        {} as FeeUpdateEventArgs
      );

      assert.strictEqual(handled, false);
    });
  });
});

describe("queueFeeUpdate", () => {
  beforeEach(() => {
    mock.method(console, "log", () => undefined);
  });

  afterEach(() => {
    mock.restoreAll();
  });

  describe("when a fee-rate update is queued but not yet replayed", () => {
    let afterQueueing: FeeConfig;

    beforeEach(async () => {
      resetOffChainMarketplaceContractData();
      seedThroughSetters(V3, FEE_COLLECTOR_SAFE);
      queueFeeUpdate(
        FeeRateUpdated.topic,
        feeRateUpdatedLog(V3, BigInt(40000), 2),
        block
      );
      afterQueueing = await getOffChainMarketplaceContractData(ctx, header, V3);
    });

    it("should leave the cache on the rate that was in force", () => {
      assert.strictEqual(afterQueueing.feeRate, BigInt(25000));
    });
  });

  describe("when the log is a fee-rate update", () => {
    let log: QueuedFeeUpdate["log"];
    let queued: QueuedFeeUpdate | null;

    beforeEach(() => {
      log = feeRateUpdatedLog(V3, BigInt(40000), 7);
      queued = queueFeeUpdate(FeeRateUpdated.topic, log, block);
    });

    it("should decode the new rate", () => {
      assert.strictEqual(
        (queued?.event as OffChainMarketplaceABI.FeeRateUpdatedEventArgs)
          ._feeRate,
        BigInt(40000)
      );
    });

    it("should keep the emitting log, which is what places it in the batch", () => {
      assert.strictEqual(queued?.log, log);
    });

    it("should keep the block", () => {
      assert.strictEqual(queued?.block, block);
    });
  });

  describe("when the log is a fee-collector update", () => {
    let queued: QueuedFeeUpdate | null;

    beforeEach(() => {
      queued = queueFeeUpdate(
        FeeCollectorUpdated.topic,
        feeCollectorUpdatedLog(V3, FEE_COLLECTOR_SAFE, 3),
        block
      );
    });

    it("should decode the new collector", () => {
      assert.strictEqual(
        (
          queued?.event as OffChainMarketplaceABI.FeeCollectorUpdatedEventArgs
        )._feeCollector.toLowerCase(),
        FEE_COLLECTOR_SAFE
      );
    });
  });

  describe("when the topic is anything else", () => {
    let queued: QueuedFeeUpdate | null;

    beforeEach(() => {
      queued = queueFeeUpdate(
        Traded.topic,
        feeRateUpdatedLog(V3, BigInt(1), 9),
        block
      );
    });

    it("should return null, leaving the log to its own handler", () => {
      assert.strictEqual(queued, null);
    });
  });
});
