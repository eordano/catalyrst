import { TradedEventArgs } from "../../abi/DecentralandMarketplaceEthereum";
import { Network } from "../../types";
import { getAddresses } from "./addresses";

export enum TradeType {
  Order = "Order",
  Bid = "Bid",
}

export enum TradeAssetType {
  ERC20 = 1,
  USD_PEGGED_MANA = 2,
  ERC721 = 3,
  ITEM = 4,
}

const MANA_PAYMENT_ASSET_TYPES = [
  TradeAssetType.ERC20,
  TradeAssetType.USD_PEGGED_MANA,
];

export const getTradeEventType = (
  event: TradedEventArgs,
  network: Network
): TradeType | undefined => {
  const addresses = getAddresses(network);

  const sent = event._trade.sent[0];
  const received = event._trade.received[0];
  if (!sent || !received) {
    return undefined;
  }

  const isReceivingMana = MANA_PAYMENT_ASSET_TYPES.includes(
    Number(received.assetType)
  );
  const isSendingMana = MANA_PAYMENT_ASSET_TYPES.includes(
    Number(sent.assetType)
  );
  const contractAddressReceived = received.contractAddress;
  const contractAddressSent = sent.contractAddress;

  if (
    isReceivingMana &&
    [addresses.MANA, addresses.TRANSAK_TOKEN].includes(contractAddressReceived)
  ) {
    return TradeType.Order;
  } else if (
    isSendingMana &&
    [addresses.MANA, addresses.TRANSAK_TOKEN].includes(contractAddressSent)
  ) {
    return TradeType.Bid;
  }
};

export const getTradeEventData = (event: TradedEventArgs, network: Network) => {
  const tradeType = getTradeEventType(event, network);
  if (tradeType === undefined) {
    return undefined;
  }
  if (tradeType === TradeType.Order) {
    return {
      collectionAddress: event._trade.sent[0].contractAddress,
      tokenId:
        Number(event._trade.sent[0].assetType) === TradeAssetType.ERC721
          ? event._trade.sent[0].value
          : undefined,
      itemId:
        Number(event._trade.sent[0].assetType) === TradeAssetType.ITEM
          ? event._trade.sent[0].value
          : undefined,
      seller: event._trade.signer,
      buyer: event._trade.sent[0].beneficiary,
      price: event._trade.received[0].value,
      assetType: event._trade.sent[0].assetType,
    };
  } else {
    return {
      collectionAddress: event._trade.received[0].contractAddress,
      tokenId:
        Number(event._trade.received[0].assetType) === TradeAssetType.ERC721
          ? event._trade.received[0].value
          : undefined,
      itemId:
        Number(event._trade.received[0].assetType) === TradeAssetType.ITEM
          ? event._trade.received[0].value
          : undefined,
      seller: event._trade.sent[0].beneficiary,
      buyer: event._trade.received[0].beneficiary,
      price: event._trade.sent[0].value,
      assetType: event._trade.received[0].assetType,
    };
  }
};
