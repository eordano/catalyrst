import { TypeormDatabase } from "@subsquid/typeorm-store";
import { run, PrometheusServer } from "@subsquid/batch-processor";
import * as evmObjects from "@subsquid/evm-objects";
import { Network } from "@dcl/schemas";
import * as landRegistryABI from "../abi/LANDRegistry";
import * as erc721abi from "../abi/ERC721";
import * as estateRegistryABI from "../abi/EstateRegistry";
import * as dclRegistrarAbi from "../abi/DCLRegistrar";
import * as marketplaceAbi from "../abi/Marketplace";
import * as erc721Bid from "../abi/ERC721Bid";
import * as dclControllerV2abi from "../abi/DCLControllerV2";
import * as MarketplaceV3ABI from "../abi/DecentralandMarketplaceEthereum";
import * as SpokeABI from "../abi/Spoke";
import { Order, Sale, Transfer, Network as ModelNetwork } from "../model";
import { dataSource, chainContext, logger, Context } from "./processor";
import { getNFTId } from "../common/utils";
import { tokenURIMutilcall } from "../common/utils/multicall";
import { getAddresses } from "../common/utils/addresses";
import {
  recordIndexingStart,
  notifyHeadReachedOnce,
} from "../common/utils/head-notification";
import {
  dropIndicesForBulkLoad,
  recreateIndices,
  checkIndicesNeedRecreation,
  logIndexConfiguration,
  isFreshSync,
  ETH_INDICES,
} from "../common/utils/indexManager";
import { getBlockRange } from "../config";
import {
  handleAddLand,
  handleCreateEstate,
  handleRemoveLand,
  handleUpdate as handleEstateUpdate,
  isAddLandEvent,
  isCreateEstateEvent,
  isRemoveLandEvent,
  isUpdateEvent,
} from "./handlers/estate";
import { handleUpdate as handleLandUpdate } from "./handlers/parcel";
import { Coordinate } from "../types";
import { getCategory } from "../common/utils/category";
import {
  addEventToStateIdsBasedOnCategory,
  getBidOwnerCutPerMillion,
  getBatchInMemoryState,
  getOwnerCutsValues,
  getMarketplaceOwnerCutPerMillion,
  setMarketplaceOwnerCutPerMillion,
  setBidOwnerCutPerMillion,
} from "./state";
import { handleNameBought, handleNameRegistered } from "./handlers/ens";
import {
  handleOrderCancelled,
  handleOrderCreated,
  handleOrderSuccessful,
  handleTraded,
} from "./handlers/marketplace";
import { getStoredData } from "./store";
import { decodeTokenIdsToCoordinates } from "./modules/land";
import {
  handleBidAccepted,
  handleBidCancelled,
  handleBidCreated,
} from "./handlers/bid";
import {
  handleAddItemV1,
  handleTransfer,
  handleTransferWearableV1,
} from "./handlers/nft";
import { getBidId } from "../common/handlers/bid";
import { handleInitializeWearablesV1 } from "./handlers/collection";
import { getItemId } from "../polygon/modules/item";
import { getWearableIdFromTokenURI } from "./modules/wearable";
import {
  getTradeEventData,
  getTradeEventType,
} from "../common/utils/marketplaceV3";

const landCoordinates: Map<bigint, Coordinate> = new Map();
const tokenURIs: Map<string, string> = new Map();

let bytesRead = 0;

//  BULK INDEX MODE: Drop the ETH-owned indices during initial sync and recreate
// them when caught up. Opt-in and default off; enable via env var BULK_INDEX_MODE=true.
// The ETH processor manages only its exclusive tables (parcel, estate, ens, data);
// tables shared with Polygon are managed by the Polygon processor. See indexManager.
const BULK_INDEX_MODE = process.env.BULK_INDEX_MODE === "true";
let bulkModeInitialized = false;
let indicesRecreated = false;
let indicesNeedRecreation = false;
// See the same pair in polygon/main.ts: recreateIndices now throws while indices are missing, so
// the head handler retries -- bounded, because an index that can never be built would otherwise
// re-attempt on every batch forever.
let indexRecreateAttempts = 0;
const MAX_INDEX_RECREATE_ATTEMPTS = 5;
const ETH_INITIAL_BLOCK = getBlockRange(Network.ETHEREUM).from;

const schemaName = process.env.DB_SCHEMA;
const db = new TypeormDatabase({
  isolationLevel: "READ COMMITTED",
  // Portal ingests from the finalized stream; a log-filtered stream yields
  // non-contiguous blocks which the hot-block path rejects. Finality on Ethereum
  // is ~15 min behind head -- acceptable for the marketplace indexer.
  supportHotBlocks: false,
  stateSchema: `eth_processor_${schemaName}`,
});
// Expose Prometheus metrics (sqd_processor_last_block / chain_height) -- the squid
// management server scrapes /metrics on this port to detect a live processor.
// setGateway used to start this; with the Portal run() we wire it explicitly.
const prometheus = new PrometheusServer();
prometheus.setPort(Number(process.env.ETH_PROMETHEUS_PORT || 3000));
run(dataSource, db, async (simpleCtx) => {
  // The batch-processor base context is bare {store, blocks, isHead}; augment the
  // blocks (restores block.logs / log.transaction back-refs) and attach `_chain`
  // (RPC for contract reads) and a logger, so the rest of the handler and the ABI
  // contract wrappers see the same shape the old evm-processor context provided.
  const ctx: Context = {
    ...simpleCtx,
    ...chainContext,
    log: logger,
    blocks: simpleCtx.blocks.map(evmObjects.augmentBlock),
  };
    bytesRead += ctx.blocks.reduce(
      (acc, block) =>
        acc +
        Buffer.byteLength(
          // BigInt-safe: Portal blocks can carry bigint fields that JSON.stringify
          // rejects; this is only a byte-count metric, so serialize them as strings.
          JSON.stringify(block, (_k, v) =>
            typeof v === "bigint" ? v.toString() : v
          ),
          "utf8"
        ),
      0
    );
    console.log("bytesRead: ", bytesRead);

    // Track indexing progress and alert Slack the first time this indexer reaches head.
    await recordIndexingStart(ctx.store, "eth");
    if (ctx.isHead && ctx.blocks.length > 0) {
      await notifyHeadReachedOnce(
        ctx.store,
        "eth",
        ctx.blocks[ctx.blocks.length - 1].header.height
      );
    }

    //  BULK INDEX MODE: check index state and drop indices on the first batch.
    if (BULK_INDEX_MODE && !bulkModeInitialized) {
      bulkModeInitialized = true;
      try {
        const currentBlock = ctx.blocks[0]?.header.height || 0;

        logIndexConfiguration(ETH_INDICES, ETH_INITIAL_BLOCK);

        const freshSync = isFreshSync(currentBlock, ETH_INITIAL_BLOCK);
        indicesNeedRecreation = await checkIndicesNeedRecreation(
          ctx.store,
          ETH_INDICES
        );

        console.log(
          `[IndexMgr] Decision (eth): block=${currentBlock.toLocaleString()}, freshSync=${freshSync}, indicesNeedRecreation=${indicesNeedRecreation}, isHead=${ctx.isHead}`
        );

        if (freshSync) {
          console.log(`[IndexMgr] Fresh sync (eth) - dropping indices for bulk indexing`);
          await dropIndicesForBulkLoad(ctx.store, ETH_INDICES);
          indicesNeedRecreation = true;
        } else if (!indicesNeedRecreation) {
          console.log(`[IndexMgr] Restart of synced squid (eth) - all indices present`);
          indicesRecreated = true;
        } else if (ctx.isHead) {
          console.log(`[IndexMgr] At head with missing indices (eth) - recreating now`);
          await recreateIndices(ctx.store, ETH_INDICES);
          indicesRecreated = true;
        } else {
          console.log(`[IndexMgr] Mid-sync restart (eth) - will recreate indices at head`);
        }
      } catch (e: any) {
        console.log(`[IndexMgr] Error in bulk index mode init (eth): ${e.message}`);
      }
    }

    //  BULK INDEX MODE: recreate indices once we reach head. MUST run before this
    // batch reads or writes any managed table -- recreateIndices issues plain
    // CREATE INDEX (SHARE lock) on an independent connection, which would deadlock
    // against ROW EXCLUSIVE locks the batch transaction takes once it starts writing.
    // recreateIndices is a no-op when nothing is missing; on error we retry next batch.
    if (
      BULK_INDEX_MODE &&
      !indicesRecreated &&
      ctx.isHead &&
      indexRecreateAttempts < MAX_INDEX_RECREATE_ATTEMPTS
    ) {
      indexRecreateAttempts++;
      console.log(`[IndexMgr] Reached chain head (eth) - recreating indices`);
      try {
        await recreateIndices(ctx.store, ETH_INDICES);
        indicesRecreated = true;
      } catch (e: any) {
        console.log(
          `[IndexMgr] Error recreating indices (eth) (attempt ${indexRecreateAttempts}/${MAX_INDEX_RECREATE_ATTEMPTS}): ${e.message}`
        );
        if (indexRecreateAttempts >= MAX_INDEX_RECREATE_ATTEMPTS) {
          console.log(
            `[IndexMgr] Giving up. The query layer is serving WITHOUT some indices \u{2014} recreate them by hand.`
          );
        }
      }
    }

    const addresses = getAddresses(Network.ETHEREUM);
    const {
      mints,
      collectionIds,
      itemIds,
      accountIds,
      estateTokenIds,
      landTokenIds,
      ensTokenIds,
      parcelEvents,
      tokenIds,
      transfers,
      bidIds,
      ensEvents,
      markteplaceEvents,
      analyticsIds,
    } = getBatchInMemoryState();

    ctx.log.info(`blocks, ${ctx.blocks.length}`);
    for (let block of ctx.blocks) {
      await getOwnerCutsValues(ctx, block);
      for (let log of block.logs) {
        const topic = log.topics[0];
        const timestamp = BigInt(block.header.timestamp / 1000);
        const analyticDayDataId = `${(
          BigInt(timestamp) / BigInt(86400)
        ).toString()}-${ModelNetwork.ETHEREUM}`;
        switch (topic) {
          case erc721abi.events[
            "Transfer(address indexed,address indexed,uint256 indexed,address,bytes,bytes)"
          ].topic:
          case erc721abi.events[
            "Transfer(address indexed,address indexed,uint256 indexed,address,bytes)"
          ].topic:
          case erc721abi.events[
            "Transfer(address indexed,address indexed,uint256 indexed)"
          ].topic:
          case erc721abi.events[
            "Transfer(address indexed,address indexed,uint256)"
          ].topic: {
            let event;
            if (
              topic ===
              erc721abi.events[
                "Transfer(address indexed,address indexed,uint256)"
              ].topic
            ) {
              event =
                erc721abi.events[
                  "Transfer(address indexed,address indexed,uint256 indexed)"
                ].decode(log);
            } else if (
              topic ===
              erc721abi.events[
                "Transfer(address indexed,address indexed,uint256 indexed)"
              ].topic
            ) {
              event =
                erc721abi.events[
                  "Transfer(address indexed,address indexed,uint256 indexed)"
                ].decode(log);
            } else if (
              topic ===
              erc721abi.events[
                "Transfer(address indexed,address indexed,uint256 indexed,address,bytes)"
              ].topic
            ) {
              event =
                erc721abi.events[
                  "Transfer(address indexed,address indexed,uint256 indexed,address,bytes)"
                ].decode(log);
            } else if (
              topic ===
              erc721abi.events[
                "Transfer(address indexed,address indexed,uint256 indexed,address,bytes,bytes)"
              ].topic
            ) {
              event =
                erc721abi.events[
                  "Transfer(address indexed,address indexed,uint256 indexed,address,bytes,bytes)"
                ].decode(log);
            }

            if (!event) {
              console.log("ERROR: event could not be decoded");
              break;
            }

            const contractAddress = log.address;
            markteplaceEvents.push({
              topic,
              event: {
                from: event.from,
                to: event.to,
                tokenId: event.tokenId,
              },
              block,
              log,
              marketplaceOwnerCutPerMillion: getMarketplaceOwnerCutPerMillion(),
              bidOwnerCutPerMillion: getBidOwnerCutPerMillion(),
            });

            accountIds.add(event.to.toString());
            switch (contractAddress) {
              case addresses.LANDRegistry:
                landTokenIds.add(event.tokenId);
                break;
              case addresses.EstateRegistry:
                estateTokenIds.add(event.tokenId);
                break;
              case addresses.DCLRegistrar:
                ensTokenIds.add(event.tokenId);
                break;
              default:
                tokenIds.set(contractAddress, [
                  ...(tokenIds.get(contractAddress) || []),
                  event.tokenId,
                ]);
                const tokenURI = tokenURIs.get(
                  `${contractAddress}-${event.tokenId}`
                );
                if (tokenURI) {
                  const representationId = getWearableIdFromTokenURI(tokenURI);
                  const itemId = getItemId(contractAddress, representationId);
                  itemIds.set(contractAddress, [
                    ...(itemIds.get(contractAddress) || []),
                    itemId,
                  ]);
                }

                break;
            }
            const category = getCategory(Network.ETHEREUM, contractAddress);
            const nftId = getNFTId(
              contractAddress,
              event.tokenId.toString(),
              category
            );
            const timestamp = block.header.timestamp / 1000;
            transfers.set(
              `${nftId}-${timestamp}`,
              new Transfer({
                id: `${nftId}-${timestamp}`,
                nftId,
                block: block.header.height,
                from: event.from,
                to: event.to,
                network: ModelNetwork.ETHEREUM,
                timestamp: BigInt(timestamp),
                txHash: log.transactionHash,
              })
            );
            break;
          }
          case erc721abi.events.OwnershipTransferred.topic: {
            markteplaceEvents.push({
              topic,
              event: erc721abi.events.OwnershipTransferred.decode(log),
              block,
              log,
            });
            break;
          }
          case erc721abi.events.AddWearable.topic: {
            collectionIds.add(log.address.toLowerCase());
            markteplaceEvents.push({
              topic,
              event: erc721abi.events.AddWearable.decode(log),
              block,
              log,
            });
            break;
          }
          case estateRegistryABI.events.CreateEstate.topic: {
            markteplaceEvents.push({
              topic,
              event: estateRegistryABI.events.CreateEstate.decode(log),
              block,
              log,
            });
            break;
          }
          case landRegistryABI.events.Update.topic:
          case estateRegistryABI.events.Update.topic: {
            if (log.address === addresses.EstateRegistry) {
              const event = estateRegistryABI.events.Update.decode(log);
              estateTokenIds.add(event._assetId);
              markteplaceEvents.push({
                topic,
                event,
                block,
                log,
              });
            } else if (log.address === addresses.LANDRegistry) {
              const event = landRegistryABI.events.Update.decode(log);
              landTokenIds.add(event.assetId);
              parcelEvents.push({
                topic,
                event,
                block,
              });
            }
            break;
          }
          case estateRegistryABI.events.AddLand.topic: {
            const event = estateRegistryABI.events.AddLand.decode(log);
            estateTokenIds.add(event._estateId);
            markteplaceEvents.push({
              topic: estateRegistryABI.events.AddLand.topic,
              event,
              block,
              log,
            });
            break;
          }
          case estateRegistryABI.events.RemoveLand.topic: {
            const event = estateRegistryABI.events.RemoveLand.decode(log);
            estateTokenIds.add(event._estateId);
            markteplaceEvents.push({
              topic: estateRegistryABI.events.RemoveLand.topic,
              event,
              block,
              log,
            });

            break;
          }
          case dclRegistrarAbi.events.NameRegistered.topic:
            ensEvents.push({
              topic,
              event: dclRegistrarAbi.events.NameRegistered.decode(log),
              block,
              log,
            });
            break;
          case dclControllerV2abi.events.NameBought.topic:
            analyticsIds.add(analyticDayDataId);
            ensEvents.push({
              topic,
              event: dclControllerV2abi.events.NameBought.decode(log),
              block,
              log,
            });

            break;
          case marketplaceAbi.events.OrderCreated.topic: {
            const event = marketplaceAbi.events.OrderCreated.decode(log);
            addEventToStateIdsBasedOnCategory(event.nftAddress, event.assetId, {
              landTokenIds,
              estateTokenIds,
              ensTokenIds,
              tokenIds,
            });

            markteplaceEvents.push({
              topic,
              event,
              block,
              log,
              marketplaceOwnerCutPerMillion: getMarketplaceOwnerCutPerMillion(),
              bidOwnerCutPerMillion: getBidOwnerCutPerMillion(),
            });
            break;
          }
          case marketplaceAbi.events.OrderSuccessful.topic: {
            const event = marketplaceAbi.events.OrderSuccessful.decode(log);
            addEventToStateIdsBasedOnCategory(event.nftAddress, event.assetId, {
              landTokenIds,
              estateTokenIds,
              ensTokenIds,
              tokenIds,
            });
            accountIds.add(event.seller);
            accountIds.add(event.buyer);

            analyticsIds.add(analyticDayDataId);
            markteplaceEvents.push({
              topic,
              event,
              block,
              log,
              marketplaceOwnerCutPerMillion: getMarketplaceOwnerCutPerMillion(),
              bidOwnerCutPerMillion: getBidOwnerCutPerMillion(),
            });
            break;
          }
          case marketplaceAbi.events.OrderCancelled.topic: {
            const event = marketplaceAbi.events.OrderCancelled.decode(log);
            addEventToStateIdsBasedOnCategory(event.nftAddress, event.assetId, {
              landTokenIds,
              estateTokenIds,
              ensTokenIds,
              tokenIds,
            });
            markteplaceEvents.push({
              topic,
              event,
              block,
              log,
              marketplaceOwnerCutPerMillion: getMarketplaceOwnerCutPerMillion(),
              bidOwnerCutPerMillion: getBidOwnerCutPerMillion(),
            });
            break;
          }
          case marketplaceAbi.events.ChangedOwnerCutPerMillion.topic:
          case erc721Bid.events.ChangedOwnerCutPerMillion.topic: {
            const event =
              marketplaceAbi.events.ChangedOwnerCutPerMillion.decode(log);
            if (log.address === addresses.Marketplace) {
              setMarketplaceOwnerCutPerMillion(event.ownerCutPerMillion);
            } else {
              setBidOwnerCutPerMillion(event.ownerCutPerMillion);
            }
            break;
          }
          case erc721Bid.events.BidCreated.topic: {
            const event = erc721Bid.events.BidCreated.decode(log);
            addEventToStateIdsBasedOnCategory(
              event._tokenAddress,
              event._tokenId,
              {
                landTokenIds,
                estateTokenIds,
                ensTokenIds,
                tokenIds,
              }
            );
            markteplaceEvents.push({
              topic: erc721Bid.events.BidCreated.topic,
              event,
              block,
              log,
              marketplaceOwnerCutPerMillion: getMarketplaceOwnerCutPerMillion(),
              bidOwnerCutPerMillion: getBidOwnerCutPerMillion(),
            });
            break;
          }
          case erc721Bid.events.BidAccepted.topic: {
            const event = erc721Bid.events.BidAccepted.decode(log);
            const bidId = getBidId(
              event._tokenAddress,
              event._tokenId.toString(),
              event._bidder
            );
            analyticsIds.add(analyticDayDataId);
            accountIds.add(event._seller);
            accountIds.add(event._bidder);
            bidIds.add(bidId);
            addEventToStateIdsBasedOnCategory(
              event._tokenAddress,
              event._tokenId,
              {
                landTokenIds,
                estateTokenIds,
                ensTokenIds,
                tokenIds,
              }
            );
            markteplaceEvents.push({
              topic: erc721Bid.events.BidAccepted.topic,
              event,
              block,
              log,
              marketplaceOwnerCutPerMillion: getMarketplaceOwnerCutPerMillion(),
              bidOwnerCutPerMillion: getBidOwnerCutPerMillion(),
            });
            break;
          }
          case erc721Bid.events.BidCancelled.topic: {
            const event = erc721Bid.events.BidCancelled.decode(log);
            const bidId = getBidId(
              event._tokenAddress,
              event._tokenId.toString(),
              event._bidder
            );
            bidIds.add(bidId);
            addEventToStateIdsBasedOnCategory(
              event._tokenAddress,
              event._tokenId,
              {
                landTokenIds,
                estateTokenIds,
                ensTokenIds,
                tokenIds,
              }
            );
            markteplaceEvents.push({
              topic: erc721Bid.events.BidCancelled.topic,
              event,
              block,
              log,
              marketplaceOwnerCutPerMillion: getMarketplaceOwnerCutPerMillion(),
              bidOwnerCutPerMillion: getBidOwnerCutPerMillion(),
            });
            break;
          }
          case MarketplaceV3ABI.events.Traded.topic: {
            const event = MarketplaceV3ABI.events.Traded.decode(log);
            const tradeData = getTradeEventData(event, Network.ETHEREUM);
            // Nothing to index: not an order or a bid (a giveaway has no payment leg).
            if (!tradeData) {
              break;
            }
            const { collectionAddress, tokenId, buyer, seller } = tradeData;

            if (!tokenId) {
              console.log(`ERROR: tokenId not found in trade event`);
              break;
            }

            addEventToStateIdsBasedOnCategory(collectionAddress, tokenId, {
              landTokenIds,
              estateTokenIds,
              ensTokenIds,
              tokenIds,
            });

            accountIds.add(seller);
            accountIds.add(buyer);
            analyticsIds.add(analyticDayDataId);

            markteplaceEvents.push({
              topic,
              event,
              block,
              log,
            });

            break;
          }
        }
      }
    }

    if (tokenIds.size) {
      console.time("multicall tokenURIs");
    }

    const tokenIdsWithoutTokenURIs = new Map<string, bigint[]>();
    for (const [contractAddress, ids] of tokenIds.entries()) {
      const newIds = new Set<bigint>();
      for (const id of ids) {
        const tokenURI = tokenURIs.get(`${contractAddress}-${id}`);
        if (!tokenURI) {
          newIds.add(id);
        }
      }
      if (newIds.size > 0) {
        tokenIdsWithoutTokenURIs.set(contractAddress, [...newIds.values()]);
      }
    }

    const newTokenURIs =
      tokenIdsWithoutTokenURIs.size > 0
        ? await tokenURIMutilcall(
            ctx,
            ctx.blocks[ctx.blocks.length - 1].header,
            tokenIdsWithoutTokenURIs
          )
        : new Map<string, string>();

    if (tokenIds.size) {
      console.timeEnd("multicall tokenURIs");
    }

    [...newTokenURIs.entries()].forEach(([contractAndTokenId, value]) => {
      const tokenURI = value;
      tokenURIs.set(contractAndTokenId, value);

      const representationId = getWearableIdFromTokenURI(tokenURI);
      const contractAddress = contractAndTokenId.split("-")[0];
      const itemId = getItemId(contractAddress, representationId);

      itemIds.set(contractAddress, [
        ...(itemIds.get(contractAddress) || []),
        itemId,
      ]);
    });

    const {
      accounts,
      datas,
      parcels,
      estates,
      nfts,
      orders,
      wearables,
      ens,
      analytics,
      counts,
      bids,
      collections,
      items,
      metadatas,
    } = await getStoredData(ctx, {
      accountIds,
      landTokenIds,
      estateTokenIds,
      ensTokenIds,
      tokenIds,
      analyticsIds,
      bidIds,
      collectionIds,
      itemIds,
    });

    const sales = new Map<string, Sale>();

    if (landTokenIds.size > 0) {
      const newCoordinates = decodeTokenIdsToCoordinates(landTokenIds);

      newCoordinates.forEach((value, key) => {
        landCoordinates.set(key, value);
      });
    }

    for (const {
      block,
      event,
      topic,
      log,
      bidOwnerCutPerMillion,
      marketplaceOwnerCutPerMillion,
    } of markteplaceEvents) {
      if (topic === marketplaceAbi.events.OrderCreated.topic) {
        handleOrderCreated(
          event as marketplaceAbi.OrderCreatedEventArgs,
          block,
          log.address,
          log.transactionHash,
          orders,
          nfts,
          counts
        );
      } else if (topic === MarketplaceV3ABI.events.Traded.topic) {
        await handleTraded(
          ctx,
          event as MarketplaceV3ABI.TradedEventArgs,
          block,
          log.transactionHash,
          nfts,
          accounts,
          analytics,
          counts,
          sales,
          items
        );
      } else if (topic === marketplaceAbi.events.OrderSuccessful.topic) {
        await handleOrderSuccessful(
          ctx,
          event as marketplaceAbi.OrderSuccessfulEventArgs,
          block,
          log.transactionHash,
          marketplaceOwnerCutPerMillion || BigInt(0),
          orders,
          nfts,
          accounts,
          analytics,
          counts,
          sales,
          items
        );
      } else if (topic === marketplaceAbi.events.OrderCancelled.topic) {
        handleOrderCancelled(
          event as marketplaceAbi.OrderCancelledEventArgs,
          block,
          nfts,
          orders
        );
      } else if (topic === erc721Bid.events.BidCreated.topic) {
        handleBidCreated(
          event as erc721Bid.BidCreatedEventArgs,
          block,
          log.address,
          nfts,
          bids
        );
      } else if (topic === erc721Bid.events.BidAccepted.topic) {
        await handleBidAccepted(
          ctx,
          event as erc721Bid.BidAcceptedEventArgs,
          block,
          log.transactionHash,
          bidOwnerCutPerMillion || BigInt(0),
          bids,
          nfts,
          accounts,
          analytics,
          counts,
          sales,
          items
        );
      } else if (topic === erc721Bid.events.BidCancelled.topic && event) {
        handleBidCancelled(
          event as erc721Bid.BidCancelledEventArgs,
          block,
          bids,
          nfts
        );
      } else if (
        topic ===
          erc721abi.events["Transfer(address indexed,address indexed,uint256)"]
            .topic ||
        topic ===
          erc721abi.events[
            "Transfer(address indexed,address indexed,uint256 indexed,address,bytes)"
          ].topic ||
        topic ===
          erc721abi.events[
            "Transfer(address indexed,address indexed,uint256 indexed,address,bytes,bytes)"
          ].topic
      ) {
        if ([...Object.values(addresses.collections)].includes(log.address)) {
          handleTransferWearableV1(
            block.header,
            log.address,
            event as erc721abi.TransferEventArgs_2,
            collections,
            items,
            orders,
            accounts,
            metadatas,
            wearables,
            counts,
            mints,
            nfts,
            tokenURIs
          );
        } else {
          handleTransfer(
            block,
            log.address,
            event as erc721abi.TransferEventArgs_2,
            accounts,
            counts,
            nfts,
            parcels,
            estates,
            wearables,
            orders,
            ens,
            tokenURIs,
            landCoordinates
          );
        }
      } else if (topic === erc721abi.events.OwnershipTransferred.topic) {
        handleInitializeWearablesV1(counts);
      } else if (topic === erc721abi.events.AddWearable.topic) {
        await handleAddItemV1(
          ctx,
          log.address,
          event as erc721abi.AddWearableEventArgs,
          block,
          collections,
          items,
          counts,
          wearables,
          metadatas
        );
      } else if (
        topic === estateRegistryABI.events.CreateEstate.topic &&
        isCreateEstateEvent(event as estateRegistryABI.CreateEstateEventArgs)
      ) {
        handleCreateEstate(
          block,
          event as estateRegistryABI.CreateEstateEventArgs,
          nfts,
          estates,
          accounts,
          datas
        );
      } else if (
        topic === estateRegistryABI.events.Update.topic &&
        isUpdateEvent(event as estateRegistryABI.UpdateEventArgs)
      ) {
        handleEstateUpdate(
          event as estateRegistryABI.UpdateEventArgs,
          block,
          estates,
          nfts,
          datas
        );
      } else if (
        topic === estateRegistryABI.events.AddLand.topic &&
        isAddLandEvent(event as estateRegistryABI.AddLandEventArgs)
      ) {
        handleAddLand(
          event as estateRegistryABI.AddLandEventArgs,
          estates,
          nfts,
          parcels,
          accounts,
          landCoordinates
        );
      } else if (
        topic === estateRegistryABI.events.RemoveLand.topic &&
        isRemoveLandEvent(event as estateRegistryABI.RemoveLandEventArgs)
      ) {
        handleRemoveLand(
          event as estateRegistryABI.RemoveLandEventArgs,
          estates,
          nfts,
          parcels,
          accounts,
          landCoordinates
        );
      }
    }

    for (const { block, event, topic } of parcelEvents) {
      if (topic === landRegistryABI.events.Update.topic) {
        handleLandUpdate(event, block, parcels, nfts, landCoordinates, datas);
      }
    }

    for (const { block, event, topic, log } of ensEvents) {
      if (topic === dclRegistrarAbi.events.NameRegistered.topic) {
        let orderHash: string | undefined = undefined;

        for (let txLog of block.logs) {
          if (
            txLog.transactionIndex === log.transactionIndex &&
            txLog.topics[0] === SpokeABI.events.OrderFilled.topic &&
            txLog.address.toLowerCase() === addresses.Spoke?.toLowerCase()
          ) {
            const orderFilledEvent = SpokeABI.events.OrderFilled.decode(txLog);
            orderHash = orderFilledEvent.orderHash;
            ctx.log.info(
              `Squid Router OrderFilled detected for ENS ${
                (event as dclRegistrarAbi.NameRegisteredEventArgs)._subdomain
              }: orderHash ${orderHash}`
            );
            break;
          }
        }

        handleNameRegistered(
          event as dclRegistrarAbi.NameRegisteredEventArgs,
          ens,
          nfts,
          accounts,
          orderHash
        );
      } else if (topic === dclControllerV2abi.events.NameBought.topic) {
        handleNameBought(
          event as dclControllerV2abi.NameBoughtEventArgs,
          BigInt(block.header.timestamp / 1000),
          analytics
        );
      }
    }

    try {
      const maps = [
        accounts,
        datas,
        estates,
        parcels,
        wearables,
        ens,
        analytics,
        counts,
        collections,
        metadatas,
        items,
      ];

      for (const entity of maps) {
        if (entity) {
          await ctx.store.upsert([...entity.values()]);
        }
      }

      // work around for circular dependency of orders and nfts
      const orderByNFT: Map<string, Order> = new Map();
      for (const nft of nfts.values()) {
        if (nft.activeOrder) {
          orderByNFT.set(nft.id, nft.activeOrder);
          nft.activeOrder = null;
        }
      }
      await ctx.store.upsert([...nfts.values()]);
      await ctx.store.upsert([...sales.values()]);
      await ctx.store.upsert([...orders.values()]);

      for (const [nftId, order] of orderByNFT) {
        const nft = nfts.get(nftId);
        if (nft) {
          nft.activeOrder = order;
        }
      }
      await ctx.store.upsert([...nfts.values()]);
      await ctx.store.upsert([...bids.values()]);
      await ctx.store.insert([...transfers.values()]);
      await ctx.store.insert([...mints.values()]);

      ctx.log.info(
        `Batch from block: ${ctx.blocks[0].header.height} to ${
          ctx.blocks[ctx.blocks.length - 1].header.height
        } saved: parcels: ${parcels.size}, nfts: ${nfts.size}, accounts: ${
          accounts.size
        }, estates: ${estates.size}, transfers: ${transfers.size}, ens: ${
          ens.size
        }. Orders: ${orders.size}, Sales: ${sales.size}, Bids: ${bids.size}`
      );
    } catch (error) {
      ctx.log.error(`error: ${error}`);
    }
  },
  { prometheus }
);
