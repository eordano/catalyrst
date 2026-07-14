import { BlockData } from "../processor";
import { Network } from "@dcl/schemas";
import { BidCancelledEventArgs } from "../../abi/ERC721Bid";
import { BidAcceptedEventArgs, BidCreatedEventArgs } from "../../abi/ERC721Bid";
import { getCategory } from "../../common/utils/category";
import { BID_SALE_TYPE, trackSale } from "../modules/analytics";
import { getNFTId } from "../../common/utils";
import {
  Bid,
  Category,
  Count,
  Item,
  NFT,
  OrderStatus,
  AnalyticsDayData,
  Sale,
  Account,
  Network as NetworkModel,
} from "../../model";
import { getAddresses } from "../../common/utils/addresses";
import { getBidId } from "../../common/handlers/bid";
import { Context } from "../processor";

export function handleBidCreated(
  event: BidCreatedEventArgs,
  block: BlockData,
  contractAddress: string,
  nfts: Map<string, NFT>,
  bids: Map<string, Bid>
): void {
  const {
    _tokenAddress,
    _tokenId,
    _bidder,
    _price,
    _fingerprint,
    _id,
    _expiresAt,
  } = event;
  const category = getCategory(Network.ETHEREUM, _tokenAddress);

  const nftId = getNFTId(
    _tokenAddress,
    _tokenId.toString(),
    category !== Category.wearable ? category : undefined
  );
  const id = getBidId(_tokenAddress, _tokenId.toString(), _bidder);

  const bid = new Bid({ id });
  const nft = nfts.get(nftId);

  if (nft) {
    bid.network = NetworkModel.ETHEREUM;
    bid.bidAddress = contractAddress;
    bid.status = OrderStatus.open;
    bid.category = category ? (category as Category) : Category.wearable;
    bid.nftAddress = _tokenAddress;
    bid.bidder = Buffer.from(_bidder.slice(2), "hex");
    bid.price = _price;
    bid.fingerprint =
      _fingerprint === "0x"
        ? Buffer.from("")
        : Buffer.from(_fingerprint.slice(2), "hex"); // hack to avoid saving 0x in hex in the db
    bid.tokenId = _tokenId;
    bid.blockchainId = _id;
    bid.blockNumber = BigInt(block.header.height);
    bid.expiresAt = _expiresAt * BigInt(1000);
    const timestamp = BigInt(block.header.timestamp / 1000);
    bid.createdAt = timestamp;
    bid.updatedAt = timestamp;

    bid.nft = nft;
    bid.seller = Buffer.from(nft.owner.address.slice(2), "hex");
    bids.set(id, bid);

    nft.updatedAt = timestamp;
  } else {
    console.log("ERROR: NFT not found for when creating bid: ", nftId);
  }
}

export async function handleBidAccepted(
  ctx: Context,
  event: BidAcceptedEventArgs,
  block: BlockData,
  txHash: string,
  ownerCutPerMillionValue: bigint,
  bids: Map<string, Bid>,
  nfts: Map<string, NFT>,
  accounts: Map<string, Account>,
  analytics: Map<string, AnalyticsDayData>,
  counts: Map<string, Count>,
  sales: Map<string, Sale>,
  items: Map<string, Item>
): Promise<void> {
  const { _bidder, _tokenAddress, _tokenId, _fee, _id, _price, _seller } =
    event;

  const id = getBidId(_tokenAddress, _tokenId.toString(), _bidder);
  const bid = bids.get(id);
  if (!bid) {
    console.log("ERROR: Bid not found on handleBidAccepted: ", id);
    return;
  }
  if (bid.nft) {
    const nft = nfts.get(bid.nft.id);
    if (!nft) {
      return;
    }

    bid.status = OrderStatus.sold;
    bid.seller = Buffer.from(_seller.slice(2), "hex");

    const timestamp = BigInt(block.header.timestamp / 1000);
    bid.blockNumber = BigInt(block.header.height);
    bid.updatedAt = timestamp;

    nft.updatedAt = timestamp;

    const addresses = getAddresses(Network.ETHEREUM);

    await trackSale(
      ctx,
      block.header,
      BID_SALE_TYPE,
      _bidder,
      _seller,
      nft.id,
      bid.price,
      ownerCutPerMillionValue,
      timestamp,
      txHash,
      nfts,
      sales,
      accounts,
      analytics,
      counts,
      items
    );
  } else {
    console.log("ERROR: NFT not found for bid in accepted: ", id);
  }
}

export function handleBidCancelled(
  event: BidCancelledEventArgs,
  block: BlockData,
  bids: Map<string, Bid>,
  nfts: Map<string, NFT>
): void {
  const { _bidder, _id, _tokenAddress, _tokenId } = event;
  const id = getBidId(_tokenAddress, _tokenId.toString(), _bidder);

  const bid = bids.get(id);
  if (!bid) {
    console.log("ERROR: Bid not found on handleBidCancelled: ", id);
    return;
  }
  if (bid.nft) {
    const nft = nfts.get(bid.nft.id);
    if (!nft) {
      console.log("ERROR: NFT not found for bid: ", bid.nft);
      return;
    }

    bid.status = OrderStatus.cancelled;
    bid.blockNumber = BigInt(block.header.height);
    const timestamp = BigInt(block.header.timestamp / 1000);
    bid.updatedAt = timestamp;

    nft.updatedAt = timestamp;
  }
}
