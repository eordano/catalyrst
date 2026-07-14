import { BlockData } from "../processor";
import { BidCancelledEventArgs } from "../abi/ERC721Bid";
import { BidAcceptedEventArgs, BidCreatedEventArgs } from "../../abi/ERC721Bid";
import { getNFTId } from "../../common/utils";
import {
  Bid,
  Category,
  Count,
  NFT,
  OrderStatus,
  Network as NetworkModel,
  SaleType,
} from "../../model";
import { buildCountFromBid } from "../../common/modules/count";
import { getBidId } from "../../common/handlers/bid";
import { trackSale } from "../modules/analytics";
import { PolygonInMemoryState, PolygonStoredData } from "../types";
import { BidV2ContractData } from "../state";
import { Context } from "../processor";

export function handleBidCreated(
  event: BidCreatedEventArgs,
  block: BlockData,
  contractAddress: string,
  nfts: Map<string, NFT>,
  bids: Map<string, Bid>,
  counts: Map<string, Count>
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

  const nftId = getNFTId(_tokenAddress, _tokenId.toString());
  const id = getBidId(_tokenAddress, _tokenId.toString(), _bidder);

  const bid = new Bid({ id });
  const nft = nfts.get(nftId);

  if (nft) {
    bid.network = NetworkModel.POLYGON;
    bid.bidAddress = contractAddress;
    bid.status = OrderStatus.open;
    bid.category = Category.wearable; // hardcoded since ethereum has this and polygon doesn't
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

  const count = buildCountFromBid(counts);
  counts.set(count.id, count);
}

export async function handleBidAccepted(
  ctx: Context,
  event: BidAcceptedEventArgs,
  block: BlockData,
  txHash: string,
  bidV2ContractData: BidV2ContractData,
  storedData: PolygonStoredData,
  inMemoryData: PolygonInMemoryState
): Promise<void> {
  const { nfts, bids } = storedData;
  const { _bidder, _tokenAddress, _tokenId, _seller } = event;

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

    if (
      bidV2ContractData.feesCollector === undefined ||
      bidV2ContractData.feesCollectorCutPerMillion === undefined ||
      bidV2ContractData.royaltiesCutPerMillion === undefined
    ) {
      console.log(
        "ERROR: feesCollector or feesCollectorCutPerMillion or royaltiesCutPerMillion not found"
      );
      return;
    }

    nft.item?.id &&
      (await trackSale(
        ctx,
        block.header,
        storedData,
        inMemoryData,
        SaleType.bid,
        _bidder,
        _seller,
        _seller,
        nft.item.id,
        nft.id,
        bid.price,
        bidV2ContractData.feesCollectorCutPerMillion,
        bidV2ContractData.feesCollector,
        bidV2ContractData.royaltiesCutPerMillion,
        timestamp,
        txHash
      ));
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
  const { _bidder, _tokenAddress, _tokenId } = event;
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
