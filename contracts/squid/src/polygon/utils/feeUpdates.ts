import * as OffChainMarketplaceABI from "../abi/DecentralandMarketplacePolygon";
import type { BlockData, Log } from "../processor";
import {
  setOffChainMarketplaceFeeCollector,
  setOffChainMarketplaceFeeRate,
  setOffChainMarketplaceRoyaltiesRate,
} from "../state";

export type FeeUpdateEventArgs =
  | OffChainMarketplaceABI.FeeCollectorUpdatedEventArgs
  | OffChainMarketplaceABI.FeeRateUpdatedEventArgs
  | OffChainMarketplaceABI.RoyaltiesRateUpdatedEventArgs;

export type QueuedFeeUpdate = {
  topic: string;
  event: FeeUpdateEventArgs;
  block: BlockData;
  log: Log & { transactionHash: string };
};

const { FeeCollectorUpdated, FeeRateUpdated, RoyaltiesRateUpdated } =
  OffChainMarketplaceABI.events;

export function queueFeeUpdate(
  topic: string,
  log: QueuedFeeUpdate["log"],
  block: BlockData
): QueuedFeeUpdate | null {
  switch (topic) {
    case FeeCollectorUpdated.topic:
      return { topic, event: FeeCollectorUpdated.decode(log), block, log };
    case FeeRateUpdated.topic:
      return { topic, event: FeeRateUpdated.decode(log), block, log };
    case RoyaltiesRateUpdated.topic:
      return { topic, event: RoyaltiesRateUpdated.decode(log), block, log };
    default:
      return null;
  }
}

export function applyFeeUpdate(
  topic: string,
  log: { address: string },
  event: FeeUpdateEventArgs
): boolean {
  switch (topic) {
    case FeeCollectorUpdated.topic:
      setOffChainMarketplaceFeeCollector(
        log.address,
        (event as OffChainMarketplaceABI.FeeCollectorUpdatedEventArgs)
          ._feeCollector
      );
      return true;
    case FeeRateUpdated.topic:
      setOffChainMarketplaceFeeRate(
        log.address,
        (event as OffChainMarketplaceABI.FeeRateUpdatedEventArgs)._feeRate
      );
      return true;
    case RoyaltiesRateUpdated.topic:
      setOffChainMarketplaceRoyaltiesRate(
        log.address,
        (event as OffChainMarketplaceABI.RoyaltiesRateUpdatedEventArgs)
          ._royaltiesRate
      );
      return true;
    default:
      return false;
  }
}
