import { BlockData } from "../processor";
import * as CollectionV2ABI from "../../polygon/abi/CollectionV2";
import * as ERC721Abi from "../../abi/ERC721";
import { AddWearableEventArgs, TransferEventArgs_2 } from "../../abi/ERC721";
import {
  Account,
  Category,
  Count,
  ENS,
  Estate,
  NFT,
  Order,
  Parcel,
  Wearable,
  Network as ModelNetwork,
  Collection,
  Item,
  Metadata,
  ItemType,
  Mint,
  OrderStatus,
} from "../../model";
import { Network } from "@dcl/schemas";
import { bigint } from "../../model/generated/marshal";
import {
  buildEstateFromNFT,
  buildParcelFromNFT,
  getAdjacentToRoad,
  getDistanceToPlaza,
  getEstateImage,
  getParcelImage,
  getParcelText,
  isInBounds,
} from "../LANDs/utils";
import { Coordinate } from "../../types";
import {
  buildWearableFromNFT,
  buildWearableV1Metadata,
  getIssuedIdFromTokenURI,
  getWearableIdFromTokenURI,
  getWearableImage,
  getWearableV1Representation,
} from "../modules/wearable";
import { buildENSFromNFT } from "../modules/ens";
import { buildCount, buildCountFromNFT } from "../modules/count";
import { getAddresses } from "../../common/utils/addresses";
import { getCategory } from "../../common/utils/category";
import {
  cancelActiveOrder,
  clearNFTOrderProperties,
  getNFTId,
  getTokenURI,
  isMint,
} from "../../common/utils";
import {
  ZERO_ADDRESS,
  createAccount,
  createOrLoadAccount,
} from "../../common/modules/account";
import {
  isWearableAccessory,
  isWearableHead,
} from "../../common/modules/metadata/wearable";
import { Block, Context } from "../processor";
import {
  getURNForCollectionV1,
  getURNForWearableV1,
} from "../../polygon/modules/metadata/wearable";
import { getItemId, getItemImage } from "../../polygon/modules/item";
import {
  setItemSearchFields,
  setNFTSearchFields,
} from "../../polygon/modules/metadata";
import { buildCountFromItem } from "../../common/modules/count";

export function handleTransfer(
  block: BlockData,
  contractAddress: string,
  event: TransferEventArgs_2,
  accounts: Map<string, Account>,
  counts: Map<string, Count>,
  nfts: Map<string, NFT>,
  parcels: Map<string, Parcel>,
  estates: Map<string, Estate>,
  wearables: Map<string, Wearable>,
  orders: Map<string, Order>,
  ensMap: Map<string, ENS>,
  tokenURIs: Map<string, string>,
  coordinates: Map<bigint, Coordinate>
): { nft?: NFT; parcel?: Parcel; account?: Account } {
  const addresses = getAddresses(Network.ETHEREUM);
  const { tokenId, to, from } = event;

  if (tokenId.toString() === "") {
    return {};
  }

  const category = getCategory(Network.ETHEREUM, contractAddress);
  const id = getNFTId(contractAddress, tokenId.toString(), category);

  let nft = nfts.get(id);

  if (!nft) {
    nft = new NFT({ id });
    nft.network = ModelNetwork.ETHEREUM;
    nfts.set(id, nft);
  }

  let toAccount = accounts.get(`${to}-${ModelNetwork.ETHEREUM}`);
  if (!toAccount) {
    toAccount = createAccount(to);
    accounts.set(`${to}-${ModelNetwork.ETHEREUM}`, toAccount);
  }

  const timestamp = BigInt(block.header.timestamp / 1000);

  nft.tokenId = tokenId;
  nft.owner = toAccount;
  nft.ownerAddress = to;
  nft.contractAddress = contractAddress;
  nft.category = category as Category;
  nft.updatedAt = timestamp;
  nft.soldAt = null;
  nft.transferredAt = timestamp;
  nft.sales = 0;
  nft.volume = bigint.fromJSON("0");

  if (
    contractAddress !== addresses.LANDRegistry &&
    contractAddress !== addresses.EstateRegistry &&
    contractAddress !== addresses.DCLRegistrar
  ) {
    // The LANDRegistry/EstateRegistry/DCLRegistrar contracts do not have a tokenURI method
    if (!nft.tokenURI) {
      nft.tokenURI = tokenURIs.get(`${contractAddress}-${tokenId}`);
    }
  } else {
    if (contractAddress === addresses.LANDRegistry) {
      nft.tokenURI = null;
    } else {
      nft.tokenURI = "";
    }
  }

  if (isMint(from)) {
    nft.createdAt = timestamp;
    // We're defaulting "Estate size" to one to allow the frontend to search for `searchEstateSize_gt: 0`,
    // necessary because thegraph doesn't support complex queries and we can't do `OR` operations
    nft.searchEstateSize = 1;
    // We default the "in bounds" property for parcels and no-parcels alike so we can just add  `searchParcelIsInBounds: true`
    // to all queries
    nft.searchParcelIsInBounds = true;
    nft.searchText = "";
    nft.searchIsLand = false;

    const metric = buildCountFromNFT(nft, counts);
    counts.set(metric.id, metric);
  } else {
    const existingNFT = nfts.get(id);
    if (existingNFT) {
      const nftActiveOrder = existingNFT.activeOrder;
      if (nftActiveOrder) {
        const order = orders.get(nftActiveOrder.id);
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
          console.log(`ERROR: Order not found ${nftActiveOrder.id}`);
        }
      }
    } else {
      console.log(`ERROR: NFT not found ${id} in handleTransfer`);
    }
  }

  if (category == Category.parcel) {
    let parcel = parcels.get(id);
    if (isMint(from)) {
      const coords = coordinates.get(tokenId);
      if (coords) {
        parcel = buildParcelFromNFT(nft, coords);
        nft.parcel = parcel;
        nft.image = getParcelImage(parcel);
        nft.searchIsLand = true;
        nft.searchParcelIsInBounds = isInBounds(parcel.x, parcel.y);
        nft.searchParcelX = parcel.x;
        nft.searchParcelY = parcel.y;
        nft.searchDistanceToPlaza = getDistanceToPlaza(parcel);
        nft.searchAdjacentToRoad = getAdjacentToRoad(parcel);
        nft.searchText = getParcelText(parcel, "");
      }
    } else {
      if (parcel) parcel.owner = nft.owner;
    }
    if (parcel) parcels.set(id, parcel);
  } else if (category == Category.estate) {
    let estate = estates.get(id);
    if (isMint(from)) {
      estate = buildEstateFromNFT(nft);
      nft.estate = estate;
      nft.image = getEstateImage(estate);
      nft.searchIsLand = true;
      nft.searchDistanceToPlaza = -1;
      nft.searchAdjacentToRoad = false;
      nft.searchEstateSize = estate.size;
    } else {
      if (estate) {
        estate.owner = nft.owner;
      } else {
        console.log(
          `ERROR: Estate with tokenId ${nft.tokenId} not found on handleTransfer. Ownership not updated correclty`
        );
      }
    }
    if (estate) {
      estates.set(id, estate);
    }
  } else if (category == Category.wearable) {
    let wearable: Wearable | undefined = undefined;
    if (isMint(from)) {
      wearable = buildWearableFromNFT(nft);
      wearable.network = ModelNetwork.ETHEREUM;
      if (!!wearable.id) {
        nft.wearable = wearable;
        nft.name = wearable.name;
        nft.image = getWearableImage(wearable);
        nft.searchIsWearableHead = isWearableHead(wearable.category);
        nft.searchIsWearableAccessory = isWearableAccessory(wearable.category);
        nft.searchWearableCategory = wearable.category;
        nft.searchWearableBodyShapes = wearable.bodyShapes;
        nft.searchWearableRarity = wearable.rarity;
        nft.searchText = wearable.name.toLowerCase();
      } else {
        console.log(`ERROR: Wearable not found ${id}`);
      }
    } else {
      const existingWearable = wearables.get(id);
      if (existingWearable) {
        wearable = new Wearable({ id: nft.id });
        wearable.network = ModelNetwork.ETHEREUM;
        wearable = existingWearable;
        wearable.owner = nft.owner;
      } else {
        console.log(`ERROR: Wearable not found ${id}`);
      }
    }
    if (wearable) wearables.set(id, wearable);
  } else if (category == Category.ens) {
    let ens: ENS | undefined = undefined;
    if (isMint(from)) {
      ens = buildENSFromNFT(nft);
      nft.ens = ens;
    } else {
      const existingENS = ensMap.get(id);

      if (existingENS) {
        ens = existingENS;
        ens.owner = nft.owner;
      } else {
        console.log(`ERROR: ENS not found ${id}`);
      }
    }
    if (ens) ensMap.set(id, ens);
  }

  createOrLoadAccount(accounts, to);

  return { nft, account: toAccount };
}

export async function handleAddItemV1(
  ctx: Context,
  collectionAddress: string,
  event: AddWearableEventArgs,
  block: BlockData,
  collections: Map<string, Collection>,
  items: Map<string, Item>,
  counts: Map<string, Count>,
  wearables: Map<string, Wearable>,
  metadatas: Map<string, Metadata>
): Promise<void> {
  const { _maxIssuance, _wearableId, _wearableIdKey } = event;
  let collection = collections.get(collectionAddress);

  const collectionContract = new CollectionV2ABI.Contract(
    ctx,
    block.header,
    collectionAddress
  );

  const owner = await collectionContract.owner();
  const timestamp = BigInt(block.header.timestamp / 1000);

  if (!collection) {
    collection = new Collection({ id: collectionAddress });
    collection.network = ModelNetwork.ETHEREUM;

    const [name, symbol, baseURI] = await Promise.all([
      collectionContract.name(),
      collectionContract.symbol(),
      collectionContract.baseURI(),
    ]);
    collection.name = name;
    collection.symbol = symbol;
    collection.owner = owner;
    collection.creator = owner;
    collection.isCompleted = true;
    collection.minters = [];
    collection.managers = [];
    collection.itemsCount = 0;
    collection.urn = getURNForCollectionV1(collection, Network.ETHEREUM);
    collection.createdAt = timestamp;
    collection.updatedAt = timestamp;
    collection.reviewedAt = timestamp;
    collection.searchIsStoreMinter = false;
    collection.searchIsMarketplaceV3Minter = false;
    collection.searchText = collection.name.toLowerCase();
    collection.isApproved = true;
    collection.usedCredits = false;
    collection.creditValue = BigInt(0);
    collection.txHash = block.header.hash;

    collection.baseURI = baseURI;
    collection.chainId = BigInt(process.env.ETHEREUM_CHAIN_ID || 1);

    collections.set(collectionAddress, collection);
  }

  collection.itemsCount += 1;

  const id = getItemId(collectionAddress, _wearableId);
  const representation = getWearableV1Representation(_wearableId);
  if (!representation) {
    console.log(
      `ERROR: No representation found for wearable ${_wearableId} in collection ${collectionAddress}`
    );
    return;
  }

  const item = new Item({ id });
  item.network = ModelNetwork.ETHEREUM;
  item.creator = owner;
  item.blockchainId = BigInt(collection.itemsCount);
  item.collection = collection;
  item.creationFee = BigInt(0);
  item.rarity = representation.rarity;
  item.available = _maxIssuance;
  item.totalSupply = BigInt(0);
  item.maxSupply = item.available;
  item.price = BigInt(0);
  item.beneficiary = ZERO_ADDRESS;
  item.rawMetadata = "";
  item.searchIsCollectionApproved = true;
  item.minters = [];
  item.managers = [];
  item.uri = (await collectionContract.baseURI()) + _wearableId;
  item.urn = getURNForWearableV1(
    collection,
    representation.id,
    Network.ETHEREUM
  );
  item.image = getItemImage(item);
  item.createdAt = timestamp;
  item.updatedAt = timestamp;
  item.reviewedAt = timestamp;
  item.searchIsStoreMinter = false;
  item.searchIsMarketplaceV3Minter = false;
  item.soldAt = null;
  item.sales = 0;
  item.volume = BigInt(0);
  item.uniqueCollectors = [];
  item.uniqueCollectorsTotal = 0;

  const metadata = buildWearableV1Metadata(
    item,
    representation!,
    wearables,
    metadatas
  );
  item.metadata = metadata;
  item.itemType = metadata.itemType;

  setItemSearchFields(item, metadatas, wearables);

  items.set(item.id, item);

  let metric = buildCountFromItem(counts, ModelNetwork.ETHEREUM);
}

export function handleTransferWearableV1(
  block: Block,
  collectionAddress: string,
  event: ERC721Abi.TransferEventArgs_2,
  collections: Map<string, Collection>,
  items: Map<string, Item>,
  orders: Map<string, Order>,
  accounts: Map<string, Account>,
  metadatas: Map<string, Metadata>,
  wearables: Map<string, Wearable>,
  counts: Map<string, Count>,
  mints: Map<string, Mint>,
  nfts: Map<string, NFT>,
  tokenURIs: Map<string, string>
): void {
  const { tokenId, from, to } = event;
  if (!tokenId.toString()) {
    return;
  }

  const collection = collections.get(collectionAddress);

  const tokenURI = tokenURIs.get(`${collectionAddress}-${tokenId}`);
  if (!tokenURI) {
    console.log(
      `ERROR: No tokenURI found for NFT ${tokenId} and collection ${collectionAddress}`
    );
    return;
  }
  const representationId = getWearableIdFromTokenURI(tokenURI);

  const itemId = getItemId(collectionAddress, representationId);
  const item = items.get(itemId);

  if (!item) {
    console.log(
      `ERROR: No item id found ${itemId} associated for NFT ${representationId}`
    );
    return;
  }

  const id = getNFTId(collectionAddress, tokenId.toString());

  const nft = nfts.get(id) || new NFT({ id, network: ModelNetwork.ETHEREUM });
  nft.collection = collection;
  nft.category = Category.wearable;
  nft.tokenId = tokenId;
  let toAccount = accounts.get(`${to}-${ModelNetwork.ETHEREUM}`);
  if (!toAccount) {
    toAccount = createAccount(to);
    accounts.set(`${to}-${ModelNetwork.ETHEREUM}`, toAccount);
  }
  nft.owner = toAccount;
  nft.ownerAddress = to;
  nft.contractAddress = collectionAddress;
  const timestamp = BigInt(block.timestamp / 1000);
  nft.updatedAt = timestamp;
  nft.soldAt = null;
  nft.transferredAt = timestamp;
  nft.itemType = ItemType.wearable_v1;
  if (!nft.tokenURI && tokenURI) {
    nft.tokenURI = tokenURI;
  }
  nft.item = item;

  nft.urn = item.urn;

  nft.sales = 0;
  nft.volume = BigInt(0);

  if (isMint(from)) {
    nft.itemBlockchainId = item.blockchainId;
    nft.issuedId = BigInt(getIssuedIdFromTokenURI(tokenURI));
    nft.wearable = buildWearableFromNFT(nft);
    if (nft.wearable) wearables.set(id, nft.wearable);
    nft.metadata = item.metadata;
    nft.itemType = item.itemType;
    nft.image = item.image;
    nft.createdAt = timestamp;

    // We're defaulting "Estate size" to one to allow the frontend to search for `searchEstateSize_gt: 0`,
    // necessary because thegraph doesn't support complex queries and we can't do `OR` operations
    nft.searchEstateSize = 1;
    // We default the "in bounds" property for parcels and no-parcels alike so we can just add  `searchParcelIsInBounds: true`
    // to all queries
    nft.searchParcelIsInBounds = true;
    nft.searchText = "";
    nft.searchIsLand = false;

    setNFTSearchFields(nft, metadatas);

    const count = buildCount(counts);
    count.nftTotal += 1;
    counts.set(count.id, count);

    item.available = item.available - BigInt(1);
    item.totalSupply = item.totalSupply + BigInt(1);

    items.set(item.id, item);

    const mint = new Mint({ id, network: ModelNetwork.ETHEREUM });
    mint.nft = nft;
    mint.item = item;

    mint.beneficiary = nft.owner.id;
    mint.creator = ZERO_ADDRESS; // v1 collections don't have a creator
    mint.minter = from;
    mint.timestamp = timestamp;
    mint.searchContractAddress = nft.contractAddress;
    mint.searchTokenId = nft.tokenId;
    mint.searchItemId = item.blockchainId;
    mint.searchIssuedId = nft.issuedId;
    mint.searchIsStoreMinter = false;
    mints.set(mint.id, mint);
  } else {
    const oldNFT = nfts.get(id);
    if (!oldNFT) {
      console.log(`ERROR: NFT not found ${id}`);
      return;
    }
    const activeOrder = oldNFT.activeOrder;
    if (activeOrder) {
      const order = orders.get(activeOrder.id);
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
      }
    }
  }

  createOrLoadAccount(accounts, to, ModelNetwork.ETHEREUM);

  nfts.set(nft.id, nft);
}
