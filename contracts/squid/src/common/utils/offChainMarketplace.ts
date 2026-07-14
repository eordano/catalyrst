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

// Payment asset types that settle in MANA: plain ERC20 MANA and the USD-pegged MANA the credits
// checkout (the Shop) uses. Both are priced in the MANA contract, so a trade whose payment leg is
// either one is a MANA sale/bid -- see getTradeEventType.
const MANA_PAYMENT_ASSET_TYPES = [
  TradeAssetType.ERC20,
  TradeAssetType.USD_PEGGED_MANA,
];

export const getTradeEventType = (
  event: TradedEventArgs,
  network: Network
): TradeType | undefined => {
  const addresses = getAddresses(network);

  // Either leg can be EMPTY. A trade that sends an asset and receives nothing is a giveaway: valid
  // on-chain (the contract accepts it and emits Traded), and neither an order nor a bid -- there is no
  // payment leg to classify. Reading `[0]` unguarded threw inside the batch transaction, which the
  // processor treats as fatal: it crash-looped on the block and stopped indexing everything behind it
  // (prod, 2026-08-07). Falling through to `undefined` is the existing "not a trade we index" answer.
  const sent = event._trade.sent[0];
  const received = event._trade.received[0];
  if (!sent || !received) {
    return undefined;
  }

  // A MANA payment leg is either plain ERC20 MANA or USD-pegged MANA (the credits/Shop checkout) -- both
  // must be recognized, otherwise a credits sale is misclassified and its collection/tokenId are read off
  // the wrong asset, so the mint never gets indexed.
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
    [addresses.MANA, addresses.TRANSAK_TOKEN].includes(contractAddressReceived) // support Transak token to track sales in dev
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
  // An unclassifiable trade used to fall through to the Bid branch, because the `else` caught both "it
  // is a bid" and "we could not tell". Those are different answers: a giveaway has no payment leg, so
  // reading a price and a seller off it invents both. Return nothing and let the callers skip it.
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
      seller: event._trade.received[0].beneficiary,
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
