import { Network } from "@dcl/schemas";
import { buildCountFromNFT } from "../../common/modules/count";
import {
  cancelActiveOrder,
  clearNFTOrderProperties,
  getNFTId,
} from "../../common/utils";
import {
  ZERO_ADDRESS,
  createOrLoadAccount,
} from "../../common/modules/account";
import { getAddresses } from "../../common/utils/addresses";
import {
  Category,
  Collection,
  Item,
  ItemType,
  Mint,
  NFT,
  Network as NetworkModel,
  OrderStatus,
  SaleType,
} from "../../model";
import { IssueEventArgs, TransferEventArgs } from "../abi/CollectionV2";
import { TradedEventArgs } from "../abi/DecentralandMarketplacePolygon";
import { setNFTSearchFields } from "../modules/metadata";
import { Block, Context } from "../processor";
import { PolygonInMemoryState, PolygonStoredData } from "../types";
import { Transaction } from "../processor";
import { trackSale } from "../modules/analytics";
import { StoreContractData } from "../state";

export async function handleMintNFT(
  ctx: Context,
  event: IssueEventArgs,
  block: Block,
  transaction: Transaction,
  collectionAddress: string,
  item: Item,
  storedData: PolygonStoredData,
  inMemoryData: PolygonInMemoryState,
  storeContractData: StoreContractData,
  tradedEvent?: TradedEventArgs
): Promise<void> {
  const { counts, collections, accounts, metadatas, nfts } = storedData;
  const { mints } = inMemoryData;
  const nftId = getNFTId(collectionAddress, event._tokenId.toString());
  const nft = new NFT({ id: nftId });

  const issuedId = event._issuedId;

  const collection = collections.get(collectionAddress);
  if (!collection) {
    console.log("ERROR: Collection not found", collectionAddress);
    return;
  }
  nft.collection = collection;
  nft.category = (
    item.itemType === ItemType.emote_v1 ? "emote" : "wearable"
  ) as Category;
  nft.tokenId = event._tokenId;
  nft.contractAddress = collectionAddress;
  nft.itemBlockchainId = event._itemId;
  nft.itemType = item.itemType;
  nft.issuedId = event._issuedId;
  nft.item = item;
  nft.urn = item.urn;
  const owner = createOrLoadAccount(
    accounts,
    event._beneficiary,
    NetworkModel.POLYGON
  );
  if (owner) {
    nft.owner = owner;
    nft.ownerAddress = owner.address;
  } else {
    console.log(
      `ERROR: Owner not found ${event._beneficiary} for NFT ${nftId}`
    );
  }
  nft.tokenURI = item.uri + "/" + issuedId.toString();
  nft.image = item.image;
  nft.metadata = item.metadata;

  const timestamp = BigInt(block.timestamp / 1000);
  nft.createdAt = timestamp;
  nft.updatedAt = timestamp;
  nft.soldAt = null;
  nft.transferredAt = timestamp;

  nft.sales = 0;
  nft.volume = BigInt(0);

  setNFTSearchFields(nft, metadatas);

  createOrLoadAccount(accounts, event._beneficiary, NetworkModel.POLYGON);

  const metric = buildCountFromNFT(nft, counts, NetworkModel.POLYGON);
  counts.set(metric.id, metric);

  nft.network = NetworkModel.POLYGON;
  nfts.set(nftId, nft);

  const minterAddress = event._caller;
  const addresses = getAddresses(Network.MATIC);
  const isStoreMinter =
    minterAddress === addresses.CollectionStore ||
    minterAddress === addresses.MarketplaceV3 ||
    minterAddress === addresses.MarketplaceV3_V2;

  const mint = new Mint({ id: nftId, network: NetworkModel.POLYGON });
  mint.nft = nft;
  mint.item = item;
  mint.beneficiary = nft.owner.id;
  mint.creator = item.creator;
  mint.minter = minterAddress;
  mint.timestamp = timestamp;
  mint.searchContractAddress = collectionAddress;
  mint.searchTokenId = nft.tokenId;
  mint.searchItemId = item.blockchainId;
  mint.searchIssuedId = issuedId;
  mint.searchIsStoreMinter = isStoreMinter;

  const fee = storeContractData.fee;
  const feeOwner = storeContractData.feeOwner;

  if (fee === undefined || feeOwner === undefined) {
    console.log("ERROR: Fee or fee owner not found");
    return;
  }

  let beneficiary =
    item.beneficiary !== ZERO_ADDRESS ? item.beneficiary : item.creator;

  const price =
    (minterAddress === addresses.MarketplaceV3 ||
      minterAddress === addresses.MarketplaceV3_V2) &&
      tradedEvent
      ? tradedEvent._trade.received[0].value
      : item.price;

  if (
    (minterAddress === addresses.MarketplaceV3 ||
      minterAddress === addresses.MarketplaceV3_V2) &&
    tradedEvent
  ) {
    beneficiary = tradedEvent._trade.received[0].beneficiary;
  }

  if (isStoreMinter) {
    mint.searchPrimarySalePrice = item.price;
    await trackSale(
      ctx,
      block,
      storedData,
      inMemoryData,
      SaleType.mint,
      event._beneficiary,
      item.creator,
      beneficiary,
      item.id,
      nft.id,
      price,
      fee,
      feeOwner,
      BigInt(0),
      timestamp,
      transaction.hash
    );
  }

  mints.set(mint.id, mint);
}

export async function handleTransferNFT(
  ctx: Context,
  collectionAddress: string,
  event: TransferEventArgs,
  block: Block,
  storedData: PolygonStoredData,
  inMemoryData: PolygonInMemoryState,
  txHash: string
): Promise<void> {
  const { nfts, orders } = storedData;
  if (event.tokenId.toString() === "") {
    return;
  }

  const id = getNFTId(collectionAddress, event.tokenId.toString());

  const nft = nfts.get(id);
  if (!nft) {
    console.log(`ERROR: NFT not found for transfer ${id}`);
    return;
  }

  const ownerAccount = createOrLoadAccount(
    storedData.accounts,
    event.to,
    NetworkModel.POLYGON
  );

  const timestamp = BigInt(block.timestamp / 1000);

  nft.owner = ownerAccount;
  nft.ownerAddress = ownerAccount.address;
  nft.updatedAt = timestamp;
  nft.transferredAt = timestamp;

  // Record this transfer as a candidate gift notification. We do NOT emit here:
  // whether it is an actual gift (vs a marketplace purchase) is decided
  // post-batch, once every Sale of the batch has been tracked. Within a single
  // transaction the ERC721 Transfer log is processed before the marketplace
  // event that records the sale, so the sale is not yet known at this point.
  inMemoryData.transferGiftCandidates.set(`${txHash}-${nft.id}`, {
    nftId: nft.id,
    from: event.from,
    to: event.to,
    tokenURI: nft.tokenURI ?? null,
    timestamp,
    txHash,
  });

  if (nft.activeOrder) {
    const order = orders.get(nft.activeOrder.id);
    if (order) {
      const isComingBackToOrderOwner = order.owner === nft.owner.address;
      order.status = isComingBackToOrderOwner
        ? OrderStatus.open
        : OrderStatus.transferred;
      if (order.status === OrderStatus.transferred) {
        nft.searchOrderStatus = OrderStatus.transferred;
      } else {
        nft.searchOrderStatus = OrderStatus.open;
      }
    } else {
      console.log(`ERROR: Order not found for NFT ${nft.id}`);
    }
  }
}
