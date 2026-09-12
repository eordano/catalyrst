import { ONE_MILLION } from "../utils/utils";
import { AnalyticsDayData, Network as ModelNetwork, Sale } from "../../model";

export let BID_SALE_TYPE = "bid";
export let ORDER_SALE_TYPE = "order";

export function getOrCreateAnalyticsDayData(
  timestamp: bigint,
  analytics: Map<string, AnalyticsDayData>,
  network: ModelNetwork = ModelNetwork.ETHEREUM
): AnalyticsDayData {
  const dayID = timestamp / BigInt(86400);
  const id = `${dayID.toString()}-${network}`;
  const dayStartTimestamp = dayID * BigInt(86400);
  let analyticsDayData = analytics.get(id);
  if (!analyticsDayData) {
    analyticsDayData = new AnalyticsDayData({
      id,
    });
    analyticsDayData.date = +dayStartTimestamp.toString();
    analyticsDayData.sales = 0;
    analyticsDayData.volume = BigInt(0);
    analyticsDayData.creatorsEarnings = BigInt(0);
    analyticsDayData.daoEarnings = BigInt(0);
    analyticsDayData.network = network;
  }
  return analyticsDayData;
}

export function updateAnalyticsDayData(
  sale: Sale,
  feesCollectorCut: bigint,
  analytics: Map<string, AnalyticsDayData>
): AnalyticsDayData {
  const analyticsDayData = getOrCreateAnalyticsDayData(
    sale.timestamp,
    analytics
  );
  analyticsDayData.sales += 1;
  analyticsDayData.volume = analyticsDayData.volume + sale.price;
  analyticsDayData.daoEarnings =
    analyticsDayData.daoEarnings +
    (feesCollectorCut * sale.price) / ONE_MILLION;

  return analyticsDayData;
}
