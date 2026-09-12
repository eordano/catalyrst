import { In, Not } from "typeorm";
import { TypeormDatabase, Store } from "@subsquid/typeorm-store";
import { Network, ChainId } from "@dcl/schemas";
import { startBlockByNetwork } from "./addresses/startBlocks";
import {
  Order,
  Rarity,
  Transfer,
  Network as ModelNetwork,
  Collection,
  Currency,
  ItemsDayData,
  SquidRouterOrder,
  NFT,
  Item,
  Metadata,
  Bid,
  Sale,
  Mint,
  Curation,
} from "../model";

import * as CollectionFactoryABI from "./abi/CollectionFactory";
import * as CollectionFactoryV3ABI from "./abi/CollectionFactoryV3";
import * as CollectionV2ABI from "./abi/CollectionV2";
import * as MarketplaceABI from "./abi/Marketplace";
import * as MarketplaceV2ABI from "./abi/MarketplaceV2";
import * as CommitteeABI from "./abi/Committee";
import * as RaritiesABI from "./abi/Rarity";
import * as OffChainMarketplaceABI from "./abi/DecentralandMarketplacePolygon";
import * as OffChainMarketplaceV3ABI from "./abi/DecentralandMarketplacePolygonV3";
import * as ERC721BidABI from "./abi/ERC721Bid";
import * as CollectionStoreABI from "./abi/CollectionStore";
import * as CollectionManagerABI from "./abi/CollectionManager";
import * as CreditsManagerABI from "./abi/CreditsManager";
import * as SpokeABI from "../abi/Spoke";
import {
  fetchCollectionDataMulticall,
  type CollectionData,
} from "./utils/collectionMulticall";
import {
  dropIndicesForBulkLoad,
  recreateIndices,
  checkIndicesNeedRecreation,
  logIndexConfiguration,
  isFreshSync,
  POLYGON_INDICES,
} from "../common/utils/indexManager";
import { getAddresses } from "../common/utils/addresses";
import {
  encodeTokenId,
  handleAddItem,
  handleCollectionCreation,
  handleCompleteCollection,
  handleIssue,
  handleRescueItem,
  handleSetApproved,
  handleSetEditable,
  handleSetGlobalManager,
  handleSetGlobalMinter,
  handleSetItemManager,
  handleSetItemMinter,
  handleTransfer,
  handleTransferCreatorship,
  handleTransferOwnership,
  handleUpdateItemData,
} from "./handlers/collection";
import { dataSource, chainContext, logger, Context } from "./processor";
import { run, PrometheusServer } from "@subsquid/batch-processor";
import * as evmObjects from "@subsquid/evm-objects";
import {
  getBatchInMemoryState,
  getBidV2ContractData,
  getMarketplaceContractData,
  getMarketplaceV2ContractData,
  getStoreContractData,
  setBidOwnerCutPerMillion,
  setMarketplaceOwnerCutPerMillion,
  setOffChainMarketplaceFeeCollector,
  setOffChainMarketplaceFeeRate,
  setOffChainMarketplaceRoyaltiesRate,
  setStoreFee,
  setStoreFeeOwner,
} from "./state";
import { getStoredData } from "./store";
import { PolygonStoredData } from "./types";
import { handleMemeberSet } from "./handlers/committee";
import { handleAddRarity, handleUpdatePrice } from "./handlers/rarity";
import { getBidId } from "../common/handlers/bid";
import {
  handleBidAccepted,
  handleBidCancelled,
  handleBidCreated,
} from "./handlers/bid";
import {
  handleOrderCancelled,
  handleOrderCreated,
  handleOrderSuccessful,
  handleTraded,
} from "./handlers/marketplace";
import { getNFTId } from "../common/utils";
import { handleRaritiesSet } from "./handlers/collectionManager";
import { loadCollections } from "./utils/loaders";
import { checkCpuUsageAndThrottle } from "../tools/os";
import {
  getTradeEventData,
  getTradeEventType,
} from "../common/utils/offChainMarketplace";
import {
  getLastNotified,
  setLastNotified,
  publishTransferGift,
} from "../common/utils/events";
import {
  recordIndexingStart,
  notifyHeadReachedOnce,
} from "../common/utils/head-notification";
import { requireDeploymentSchema } from "../common/utils/deployment-schema";

const schemaName = requireDeploymentSchema();
const addresses = getAddresses(Network.MATIC);
let bytesRead = 0;

const creditsManagerAddresses = new Set(
  addresses.CreditsManager.map((a: string) => a.toLowerCase())
);
const collectionFactoryAddresses = new Set(
  [addresses.CollectionFactory, addresses.CollectionFactoryV3].map((c) =>
    c.toLowerCase()
  )
);
const spokeAddressLower = addresses.Spoke?.toLowerCase();

const fmt = (ms: number) =>
  ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms)}ms`;

function pushToMapArray<K, V>(map: Map<K, V[]>, key: K, value: V): void {
  let arr = map.get(key);
  if (!arr) {
    arr = [];
    map.set(key, arr);
  }
  arr.push(value);
}

const topicToName: Record<string, string> = {
  [CollectionFactoryABI.events.ProxyCreated.topic]: "ProxyCreated",
  [CollectionFactoryV3ABI.events.ProxyCreated.topic]: "ProxyCreatedV3",
  [MarketplaceABI.events.OrderCreated.topic]: "OrderCreated",
  [MarketplaceABI.events.OrderSuccessful.topic]: "OrderSuccessful",
  [MarketplaceABI.events.OrderCancelled.topic]: "OrderCancelled",
  [ERC721BidABI.events.BidCreated.topic]: "BidCreated",
  [ERC721BidABI.events.BidAccepted.topic]: "BidAccepted",
  [ERC721BidABI.events.BidCancelled.topic]: "BidCancelled",
  [CollectionV2ABI.events.Transfer.topic]: "Transfer",
  [CollectionV2ABI.events.Issue.topic]: "Issue",
  [CollectionV2ABI.events.AddItem.topic]: "AddItem",
  [OffChainMarketplaceABI.events.Traded.topic]: "Traded",
  [OffChainMarketplaceV3ABI.events.Traded.topic]: "Traded",
};
const preloadedCollections = loadCollections().addresses;
const preloadedCollectionsSet = new Set(preloadedCollections);
const preloadedCollectionsHeight = loadCollections().height;
let cachedLastNotified: bigint | null = null;
let lastNotifiedLoaded = false;
const minTransferNotificationTimestamp = BigInt(
  process.env.MIN_TRANSFER_NOTIFICATION_TIMESTAMP ??
    Math.floor(Date.now() / 1000)
);

const BULK_INDEX_MODE = process.env.BULK_INDEX_MODE === "true";
let bulkModeInitialized = false;
let indicesRecreated = false;
let indicesNeedRecreation = false;
let indexRecreateAttempts = 0;
const MAX_INDEX_RECREATE_ATTEMPTS = 5;

const chainId = +(process.env.POLYGON_CHAIN_ID || ChainId.MATIC_MAINNET);
const networkStartBlocks = startBlockByNetwork[chainId] || startBlockByNetwork[ChainId.MATIC_MAINNET];
const INITIAL_BLOCK = Math.min(...Object.values(networkStartBlocks));

let totalEventsProcessed = 0;

interface UpsertResult {
  timing: {
    phase1: number;
    metadatas: number;
    items: number;
    nfts1: number;
    orders: number;
    phase4: number;
    total: number;
  };
  nftsWithOrdersCount: number;
}

async function performUpserts(
  store: Store,
  fmt: (ms: number) => string,
  storedData: PolygonStoredData,
  rarities: Map<string, Rarity>,
  metadatas: Map<string, Metadata>,
  items: Map<string, Item>,
  nfts: Map<string, NFT>,
  orders: Map<string, Order>,
  bids: Map<string, Bid>,
  sales: Map<string, Sale>,
  mints: Map<string, Mint>,
  transfers: Map<string, Transfer>,
  curations: Map<string, Curation>,
  squidRouterOrders: Map<string, SquidRouterOrder>
): Promise<UpsertResult> {
  const upsertStart = performance.now();
  const timing = {
    phase1: 0,
    metadatas: 0,
    items: 0,
    nfts1: 0,
    orders: 0,
    phase4: 0,
    total: 0,
  };

  let t0 = performance.now();
  await store.upsert([...rarities.values()]);
  await store.upsert([...storedData.counts.values()]);
  await store.upsert([...storedData.accounts.values()]);
  await store.upsert([...storedData.collections.values()]);
  await store.upsert([...storedData.analytics.values()]);
  await store.upsert([...storedData.itemDayDatas.values()]);
  await store.upsert([...storedData.accountsDayDatas.values()]);
  await store.upsert([...storedData.wearables.values()]);
  await store.upsert([...storedData.emotes.values()]);
  timing.phase1 = performance.now() - t0;

  t0 = performance.now();
  await store.upsert([...metadatas.values()]);
  timing.metadatas = performance.now() - t0;

  t0 = performance.now();
  await store.upsert([...items.values()]);
  timing.items = performance.now() - t0;

  const orderByNFT: Map<string, Order> = new Map();
  for (const nft of nfts.values()) {
    if (nft.activeOrder) {
      orderByNFT.set(nft.id, nft.activeOrder);
      nft.activeOrder = null;
    }
  }

  t0 = performance.now();
  await store.upsert([...nfts.values()]);
  timing.nfts1 = performance.now() - t0;

  t0 = performance.now();
  await store.upsert([...orders.values()]);
  timing.orders = performance.now() - t0;

  const nftsWithOrders: NFT[] = [];
  for (const [nftId, order] of orderByNFT) {
    const nft = nfts.get(nftId);
    if (nft) {
      nft.activeOrder = order;
      nftsWithOrders.push(nft);
    }
  }

  t0 = performance.now();
  if (nftsWithOrders.length > 0) await store.upsert(nftsWithOrders);
  await store.upsert([...bids.values()]);
  await store.insert([...sales.values()]);
  await store.insert([...mints.values()]);
  await store.insert([...transfers.values()]);
  await store.insert([...curations.values()]);
  await store.insert([...squidRouterOrders.values()]);
  timing.phase4 = performance.now() - t0;

  timing.total = performance.now() - upsertStart;

  if (timing.total > 2000) {
    const nftSpeed =
      nfts.size > 0 ? (timing.nfts1 / nfts.size).toFixed(2) : "0";
    console.log(
      `\u{1F4BE} Upsert: phase1=${fmt(timing.phase1)}, metadatas=${fmt(
        timing.metadatas
      )}, items=${fmt(timing.items)}, nfts1=${fmt(timing.nfts1)}(${
        nfts.size
      } @ ${nftSpeed}ms/nft), orders=${fmt(timing.orders)}, phase4=${fmt(
        timing.phase4
      )}(${nftsWithOrders.length} w/orders)`
    );
  }

  return { timing, nftsWithOrdersCount: nftsWithOrders.length };
}

const db = new TypeormDatabase({
  isolationLevel: "READ COMMITTED",
  supportHotBlocks: false,
  stateSchema: `polygon_processor_${schemaName}`,
});
const prometheus = new PrometheusServer();
prometheus.setPort(Number(process.env.POLYGON_PROMETHEUS_PORT || 3001));
run(dataSource, db, async (simpleCtx) => {
  const ctx: Context = {
    ...simpleCtx,
    ...chainContext,
    log: logger,
    blocks: simpleCtx.blocks.map(evmObjects.augmentBlock),
  };
    const batchStartTime = performance.now();
    const metrics = {
      blockRange: `${ctx.blocks[0].header.height}-${
        ctx.blocks[ctx.blocks.length - 1].header.height
      }`,
      eventsProcessed: 0,
      rpcCalls: { owner: 0, items: 0, contractData: 0, rarity: 0 },
      rpcTime: { owner: 0, items: 0, rarity: 0, total: 0 },
      dbQueryTime: 0,
      eventLoopTime: 0,
      upsertTime: 0,
      preIndexTime: 0,
      ownerMulticallTime: 0,
      eventLoopBreakdown: {
        proxyCreated: 0,
        orderEvents: 0,
        bidEvents: 0,
        transferEvents: 0,
        collectionEvents: 0,
        committeeEvents: 0,
        tradedEvents: 0,
        otherEvents: 0,
      },
    };

    bytesRead += ctx.blocks.reduce(
      (acc, block) =>
        acc +
        Buffer.byteLength(
          JSON.stringify(block, (_k, v) =>
            typeof v === "bigint" ? v.toString() : v
          ),
          "utf8"
        ),
      0
    );

    await recordIndexingStart(ctx.store, "polygon");
    if (ctx.isHead && ctx.blocks.length > 0) {
      await notifyHeadReachedOnce(
        ctx.store,
        "polygon",
        ctx.blocks[ctx.blocks.length - 1].header.height
      );
    }

    if (BULK_INDEX_MODE && !bulkModeInitialized) {
      bulkModeInitialized = true;
      try {
        const currentBlock = ctx.blocks[0]?.header.height || 0;

        logIndexConfiguration(POLYGON_INDICES, INITIAL_BLOCK);

        const freshSync = isFreshSync(currentBlock, INITIAL_BLOCK);
        indicesNeedRecreation = await checkIndicesNeedRecreation(
          ctx.store,
          POLYGON_INDICES
        );

        console.log(
          `[IndexMgr] Decision: block=${currentBlock.toLocaleString()}, freshSync=${freshSync}, indicesNeedRecreation=${indicesNeedRecreation}, isHead=${ctx.isHead}`
        );

        if (freshSync) {
          console.log(`[IndexMgr] Fresh sync - dropping indices for bulk indexing`);
          await dropIndicesForBulkLoad(ctx.store, POLYGON_INDICES);
          indicesNeedRecreation = true;
        } else if (!indicesNeedRecreation) {
          console.log(`[IndexMgr] Restart of synced squid - all indices present`);
          indicesRecreated = true;
        } else if (ctx.isHead) {
          console.log(`[IndexMgr] At head with missing indices - recreating now`);
          await recreateIndices(ctx.store, POLYGON_INDICES);
          indicesRecreated = true;
        } else {
          console.log(`[IndexMgr] Mid-sync restart - will recreate indices at head`);
        }
      } catch (e: any) {
        console.log(`[IndexMgr] Error in bulk index mode init: ${e.message}`);
      }
    }

    if (
      BULK_INDEX_MODE &&
      !indicesRecreated &&
      ctx.isHead &&
      indexRecreateAttempts < MAX_INDEX_RECREATE_ATTEMPTS
    ) {
      indexRecreateAttempts++;
      console.log(`[IndexMgr] Reached chain head - recreating indices`);
      try {
        await recreateIndices(ctx.store, POLYGON_INDICES);
        indicesRecreated = true;
      } catch (e: any) {
        console.log(
          `[IndexMgr] Error recreating indices (attempt ${indexRecreateAttempts}/${MAX_INDEX_RECREATE_ATTEMPTS}): ${e.message}`
        );
        if (indexRecreateAttempts >= MAX_INDEX_RECREATE_ATTEMPTS) {
          console.log(
            `[IndexMgr] Giving up. The query layer is serving WITHOUT some indices \u{2014} recreate them by hand.`
          );
        }
      }
    }

    const [rarities, collectionIdsNotIncludedInPreloaded] = await Promise.all([
      ctx.store.find(Rarity).then((q) => new Map(q.map((i) => [i.id, i]))),
      ctx.store
        .find(Collection, {
          where: {
            id: Not(In(preloadedCollections)),
            network: ModelNetwork.POLYGON,
          },
        })
        .then((q) => new Set(q.map((c) => c.id))),
    ]);

    const isThereImportantDataInBatch = ctx.blocks.some((block) =>
      block.logs.some(
        (log) =>
          log.address === addresses.CollectionFactory ||
          log.address === addresses.CollectionFactoryV3 ||
          log.address === addresses.BidV2 ||
          log.address === addresses.ERC721Bid ||
          log.address === addresses.Marketplace ||
          log.address === addresses.MarketplaceV2 ||
          log.address === addresses.OldCommittee ||
          log.address === addresses.Committee ||
          log.address === addresses.CollectionStore ||
          log.address === addresses.RaritiesWithOracle ||
          log.address === addresses.Rarity ||
          log.address === addresses.CollectionManager ||
          log.address === addresses.OffChainMarketplace ||
          log.address === addresses.OffChainMarketplaceV2 ||
          log.address === addresses.OffChainMarketplaceV3 ||
          preloadedCollectionsSet.has(log.address) ||
          collectionIdsNotIncludedInPreloaded.has(log.address)
      )
    );

    if (
      !isThereImportantDataInBatch &&
      ctx.blocks[ctx.blocks.length - 1].header.height >
        preloadedCollectionsHeight
    ) {
      console.log(
        "INFO: Batch contains important data: ",
        isThereImportantDataInBatch
      );
      return;
    }

    const collectionIdsCreatedInBatch = new Set<string>();
    const inMemoryData = getBatchInMemoryState();
    const {
      sales,
      curations,
      mints,
      squidRouterOrders,
      transferGiftCandidates,
      itemIds,
      collectionIds,
      accountIds,
      tokenIds,
      transfers,
      bidIds,
      analyticsIds,
      itemDayDataIds,
      events,
      collectionFactoryEvents,
      committeeEvents,
    } = inMemoryData;

    ctx.log.info(
      `blocks, amount: ${ctx.blocks.length}, from: ${
        ctx.blocks[0].header.height
      } to: ${ctx.blocks[ctx.blocks.length - 1].header.height}`
    );

    if (!lastNotifiedLoaded) {
      cachedLastNotified = await getLastNotified(ctx.store);
      lastNotifiedLoaded = true;
      console.log("Loaded lastNotified timestamp:", cachedLastNotified);
    }

    const lastBlockTimestamp = BigInt(
      ctx.blocks[ctx.blocks.length - 1].header.timestamp / 1000
    );

    const isProcessingNewBlocks =
      cachedLastNotified === null || lastBlockTimestamp > cachedLastNotified;

    let batchLastNotified: bigint | null | undefined = null;
    if (isProcessingNewBlocks) {
      batchLastNotified = await getLastNotified(ctx.store);
      cachedLastNotified = batchLastNotified;
    }

    const lastBlockHeader = ctx.blocks[ctx.blocks.length - 1].header;
    const [
      cachedMarketplaceData,
      cachedMarketplaceV2Data,
      cachedBidV2Data,
      cachedStoreData,
    ] = await Promise.all([
      getMarketplaceContractData(ctx, lastBlockHeader),
      getMarketplaceV2ContractData(ctx, lastBlockHeader),
      getBidV2ContractData(ctx, lastBlockHeader),
      getStoreContractData(ctx, lastBlockHeader),
    ]);

    const preIndexStart = performance.now();

    const creditEventsByTx = new Map<
      string,
      { creditId: string; value: bigint }[]
    >();
    const orderHashByTx = new Map<string, string>();
    const proxyCreatedEvents: { address: string; blockHeader: any }[] = [];

    for (let block of ctx.blocks) {
      for (let log of block.logs) {
        const topic = log.topics[0];
        const logAddressLower = log.address.toLowerCase();

        if (
          topic === CreditsManagerABI.events.CreditUsed.topic &&
          creditsManagerAddresses.has(logAddressLower)
        ) {
          const txKey = `${block.header.height}-${log.transactionIndex}`;
          const creditEvent = CreditsManagerABI.events.CreditUsed.decode(log);
          let credits = creditEventsByTx.get(txKey);
          if (!credits) {
            credits = [];
            creditEventsByTx.set(txKey, credits);
          }
          credits.push({
            creditId: creditEvent._creditId,
            value: creditEvent._value,
          });
        }

        if (
          topic === SpokeABI.events.OrderCreated.topic &&
          logAddressLower === spokeAddressLower
        ) {
          const txKey = `${block.header.height}-${log.transactionIndex}`;
          const orderCreatedEvent = SpokeABI.events.OrderCreated.decode(log);
          orderHashByTx.set(txKey, orderCreatedEvent.orderHash);
        }

        if (
          (topic === CollectionFactoryABI.events.ProxyCreated.topic ||
            topic === CollectionFactoryV3ABI.events.ProxyCreated.topic) &&
          collectionFactoryAddresses.has(logAddressLower)
        ) {
          const event =
            topic === CollectionFactoryABI.events.ProxyCreated.topic
              ? CollectionFactoryABI.events.ProxyCreated.decode(log)
              : CollectionFactoryV3ABI.events.ProxyCreated.decode(log);

          proxyCreatedEvents.push({
            address: event._address,
            blockHeader: block.header,
          });
        }
      }
    }

    metrics.preIndexTime = performance.now() - preIndexStart;

    let prefetchedCollectionData = new Map<string, CollectionData>();

    if (proxyCreatedEvents.length > 0) {
      const multicallStart = performance.now();

      const lastBlock = ctx.blocks[ctx.blocks.length - 1].header;
      const collectionAddresses = proxyCreatedEvents.map((e) => e.address);

      prefetchedCollectionData = await fetchCollectionDataMulticall(
        ctx,
        lastBlock,
        collectionAddresses
      );

      const multicallDuration = performance.now() - multicallStart;
      metrics.ownerMulticallTime = multicallDuration;
      metrics.rpcTime.owner = multicallDuration;
      metrics.rpcTime.total += multicallDuration;
      metrics.rpcCalls.owner = proxyCreatedEvents.length;
    }

    const eventTypeCounts: Record<string, number> = {};
    const eventTypeTimes: Record<string, number> = {};
    const accumulationLoopStart = performance.now();

    let skippedTransfers = 0;
    let processedTransfers = 0;

    let preEventTime = 0;

    let currentRaritiesSnapshot: Map<string, Rarity> = new Map(
      Array.from(rarities).map(([k, v]) => [k, { ...v } as Rarity])
    );
    let raritiesSnapshotDirty = false;

    const validCollections = new Set<string>([
      ...preloadedCollections,
      ...collectionIdsNotIncludedInPreloaded,
    ]);

    for (let block of ctx.blocks) {
      const blockTimestamp = BigInt(block.header.timestamp / 1000);
      const dayId = (blockTimestamp / BigInt(86400)).toString();

      for (let log of block.logs) {
        const topic = log.topics[0];

        if (topic === CollectionV2ABI.events.Transfer.topic) {
          if (!validCollections.has(log.address)) {
            skippedTransfers++;
            continue;
          }
        }

        const preStart = performance.now();
        const analyticDayDataId = `${dayId}-${ModelNetwork.POLYGON}`;
        metrics.eventsProcessed++;
        preEventTime += performance.now() - preStart;

        const eventStart = performance.now();

        switch (topic) {
          case CollectionFactoryABI.events.ProxyCreated.topic:
          case CollectionFactoryV3ABI.events.ProxyCreated.topic: {
            if (
              ![addresses.CollectionFactory, addresses.CollectionFactoryV3]
                .map((c) => c.toLowerCase())
                .includes(log.address)
            ) {
              ctx.log.warn(
                `CollectionFactory event found not from collection factory contract: ${log.address}`
              );
              break;
            }

            const event =
              topic === CollectionFactoryABI.events.ProxyCreated.topic
                ? CollectionFactoryABI.events.ProxyCreated.decode(log)
                : CollectionFactoryV3ABI.events.ProxyCreated.decode(log);

            collectionIdsCreatedInBatch.add(event._address.toLowerCase());
            validCollections.add(event._address.toLowerCase());

            const prefetched = prefetchedCollectionData.get(
              event._address.toLowerCase()
            );

            let owner: string;
            if (prefetched) {
              owner = prefetched.owner;
            } else {
              const collectionContract = new CollectionV2ABI.Contract(
                ctx,
                block.header,
                event._address
              );
              const rpcStart = performance.now();
              owner = (await collectionContract.owner()).toLowerCase();
              const rpcDuration = performance.now() - rpcStart;
              metrics.rpcCalls.owner++;
              metrics.rpcTime.owner += rpcDuration;
              metrics.rpcTime.total += rpcDuration;
            }

            accountIds.add(owner);
            collectionIds.add(event._address.toLowerCase());

            const txKey = `${block.header.height}-${log.transactionIndex}`;
            const creditEvents = creditEventsByTx.get(txKey) || [];
            const orderHash = orderHashByTx.get(txKey);

            const usedCredits = creditEvents.length > 0;
            const creditValue = creditEvents.reduce(
              (sum, c) => sum + c.value,
              BigInt(0)
            );
            if (usedCredits) {
              ctx.log.info(
                `Credits detected for collection ${event._address}: ${creditValue} wei (collection creation, not cross-chain)`
              );
            }

            collectionFactoryEvents.push({
              event:
                topic === CollectionFactoryABI.events.ProxyCreated.topic
                  ? CollectionFactoryABI.events.ProxyCreated.decode(log)
                  : CollectionFactoryV3ABI.events.ProxyCreated.decode(log),
              block,
              usedCredits,
              creditValue: usedCredits ? creditValue : undefined,
              txHash: log.transactionHash,
            });

            break;
          }
          case MarketplaceABI.events.OrderCreated.topic:
          case MarketplaceV2ABI.events.OrderCreated.topic:
            if (
              ![addresses.Marketplace, addresses.MarketplaceV2]
                .map((c) => c.toLowerCase())
                .includes(log.address)
            ) {
              ctx.log.warn(
                "Marketplace event found not from marketplace contract"
              );
              break;
            }

            const event = MarketplaceABI.events.OrderCreated.decode(log);
            pushToMapArray(tokenIds, event.nftAddress, event.assetId);

            events.push({
              topic,
              event,
              block,
              log,
              marketplaceContractData: cachedMarketplaceData,
              marketplaceV2ContractData: cachedMarketplaceV2Data,
              bidV2ContractData: cachedBidV2Data,
            });
            break;

          case MarketplaceABI.events.OrderSuccessful.topic:
          case MarketplaceV2ABI.events.OrderSuccessful.topic: {
            if (
              ![addresses.Marketplace, addresses.MarketplaceV2]
                .map((c) => c.toLowerCase())
                .includes(log.address)
            ) {
              ctx.log.warn(
                `Marketplace event found not from marketplace contract`
              );
              break;
            }
            const event = MarketplaceABI.events.OrderSuccessful.decode(log);
            pushToMapArray(tokenIds, event.nftAddress, event.assetId);
            accountIds.add(event.seller);
            accountIds.add(event.buyer);
            analyticsIds.add(analyticDayDataId);
            const dayID = blockTimestamp / BigInt(86400);
            const nftId = `${event.nftAddress}-${event.assetId}`;
            const tempItemDayDataId = `${dayID.toString()}-nft-${nftId}`;
            itemDayDataIds.add(tempItemDayDataId);
            events.push({
              topic,
              event,
              block,
              log,
              marketplaceContractData: cachedMarketplaceData,
              marketplaceV2ContractData: cachedMarketplaceV2Data,
              bidV2ContractData: cachedBidV2Data,
            });
            break;
          }

          case MarketplaceABI.events.OrderCancelled.topic:
          case MarketplaceV2ABI.events.OrderCancelled.topic: {
            if (
              ![addresses.Marketplace, addresses.MarketplaceV2]
                .map((c) => c.toLowerCase())
                .includes(log.address)
            ) {
              break;
            }
            const event = MarketplaceABI.events.OrderCancelled.decode(log);
            pushToMapArray(tokenIds, event.nftAddress, event.assetId);
            events.push({
              topic,
              event,
              block,
              log,
              marketplaceContractData: cachedMarketplaceData,
              marketplaceV2ContractData: cachedMarketplaceV2Data,
              bidV2ContractData: cachedBidV2Data,
            });
            break;
          }
          case ERC721BidABI.events.BidCreated.topic: {
            const event = ERC721BidABI.events.BidCreated.decode(log);
            pushToMapArray(tokenIds, event._tokenAddress, event._tokenId);

            events.push({
              topic: ERC721BidABI.events.BidCreated.topic,
              event,
              block,
              log,
              marketplaceContractData: cachedMarketplaceData,
              marketplaceV2ContractData: cachedMarketplaceV2Data,
              bidV2ContractData: cachedBidV2Data,
            });
            break;
          }
          case ERC721BidABI.events.BidAccepted.topic: {
            const event = ERC721BidABI.events.BidAccepted.decode(log);
            const bidId = getBidId(
              event._tokenAddress,
              event._tokenId.toString(),
              event._bidder
            );
            accountIds.add(event._seller);
            accountIds.add(event._bidder);
            bidIds.add(bidId);
            pushToMapArray(tokenIds, event._tokenAddress, event._tokenId);
            analyticsIds.add(analyticDayDataId);
            const dayIDBid = blockTimestamp / BigInt(86400);
            const nftIdBid = `${event._tokenAddress}-${event._tokenId}`;
            const tempItemDayDataIdBid = `${dayIDBid.toString()}-nft-${nftIdBid}`;
            itemDayDataIds.add(tempItemDayDataIdBid);
            events.push({
              topic: ERC721BidABI.events.BidAccepted.topic,
              event,
              block,
              log,
              marketplaceContractData: cachedMarketplaceData,
              marketplaceV2ContractData: cachedMarketplaceV2Data,
              bidV2ContractData: cachedBidV2Data,
            });
            break;
          }
          case ERC721BidABI.events.BidCancelled.topic: {
            const event = ERC721BidABI.events.BidCancelled.decode(log);
            const bidId = getBidId(
              event._tokenAddress,
              event._tokenId.toString(),
              event._bidder
            );
            bidIds.add(bidId);
            pushToMapArray(tokenIds, event._tokenAddress, event._tokenId);
            events.push({
              topic: ERC721BidABI.events.BidCancelled.topic,
              event,
              block,
              log,
              marketplaceContractData: cachedMarketplaceData,
              marketplaceV2ContractData: cachedMarketplaceV2Data,
              bidV2ContractData: cachedBidV2Data,
            });
            break;
          }
          case OffChainMarketplaceABI.events.FeeCollectorUpdated.topic: {
            setOffChainMarketplaceFeeCollector(
              OffChainMarketplaceABI.events.FeeCollectorUpdated.decode(log)._feeCollector
            );
            break;
          }
          case OffChainMarketplaceABI.events.FeeRateUpdated.topic: {
            setOffChainMarketplaceFeeRate(
              OffChainMarketplaceABI.events.FeeRateUpdated.decode(log)._feeRate
            );
            break;
          }
          case OffChainMarketplaceABI.events.RoyaltiesRateUpdated.topic: {
            setOffChainMarketplaceRoyaltiesRate(
              OffChainMarketplaceABI.events.RoyaltiesRateUpdated.decode(log)._royaltiesRate
            );
            break;
          }
          case MarketplaceV2ABI.events.ChangedFeesCollectorCutPerMillion.topic:
          case ERC721BidABI.events.ChangedOwnerCutPerMillion.topic: {
            if (log.address === addresses.Marketplace) {
              const event =
                MarketplaceV2ABI.events.ChangedFeesCollectorCutPerMillion.decode(
                  log
                );
              setMarketplaceOwnerCutPerMillion(
                event.feesCollectorCutPerMillion
              );
            } else {
              const event =
                ERC721BidABI.events.ChangedOwnerCutPerMillion.decode(log);
              setBidOwnerCutPerMillion(event._ownerCutPerMillion);
            }
            break;
          }
          case CommitteeABI.events.MemberSet.topic: {
            if (
              ![addresses.Committee, addresses.OldCommittee]
                .map((c) => c.toLowerCase())
                .includes(log.address)
            ) {
              console.log(
                "ERROR: Committee event found not from committee contract"
              );
              break;
            }
            const event = CommitteeABI.events.MemberSet.decode(log);
            committeeEvents.push(event);
            accountIds.add(event._member.toLowerCase());
            break;
          }
          case CollectionV2ABI.events.SetGlobalMinter.topic:
          case CollectionV2ABI.events.SetGlobalManager.topic:
          case CollectionV2ABI.events.SetItemMinter.topic:
          case CollectionV2ABI.events.SetItemManager.topic:
          case CollectionV2ABI.events.AddItem.topic:
          case CollectionV2ABI.events.RescueItem.topic:
          case CollectionV2ABI.events.UpdateItemData.topic:
          case CollectionV2ABI.events.Issue.topic:
          case CollectionV2ABI.events.SetApproved.topic:
          case CollectionV2ABI.events.SetEditable.topic:
          case CollectionV2ABI.events.Complete.topic:
          case CollectionV2ABI.events.CreatorshipTransferred.topic:
          case CollectionV2ABI.events.OwnershipTransferred.topic:
          case CollectionV2ABI.events.Transfer.topic: {
            if (!validCollections.has(log.address)) {
              break;
            }
            processedTransfers++;
            let event;

            switch (topic) {
              case CollectionV2ABI.events.SetGlobalMinter.topic:
                event = CollectionV2ABI.events.SetGlobalMinter.decode(log);
                break;
              case CollectionV2ABI.events.SetGlobalManager.topic:
                event = CollectionV2ABI.events.SetGlobalManager.decode(log);
                break;
              case CollectionV2ABI.events.SetItemMinter.topic:
                event = CollectionV2ABI.events.SetItemMinter.decode(log);
                break;
              case CollectionV2ABI.events.SetItemManager.topic:
                event = CollectionV2ABI.events.SetItemManager.decode(log);
                break;
              case CollectionV2ABI.events.AddItem.topic:
                event = CollectionV2ABI.events.AddItem.decode(log);
                analyticsIds.add(analyticDayDataId);
                break;
              case CollectionV2ABI.events.RescueItem.topic:
                event = CollectionV2ABI.events.RescueItem.decode(log);
                break;
              case CollectionV2ABI.events.UpdateItemData.topic:
                event = CollectionV2ABI.events.UpdateItemData.decode(log);
                pushToMapArray(itemIds, log.address, event._itemId);
                break;
              case CollectionV2ABI.events.Issue.topic: {
                event = CollectionV2ABI.events.Issue.decode(log);
                accountIds.add(event._beneficiary.toLowerCase());
                analyticsIds.add(analyticDayDataId);
                const dayID = blockTimestamp / BigInt(86400);
                const itemId = `${log.address}-${event._itemId}`;
                const itemDayDataId = `${dayID.toString()}-${itemId}`;
                itemDayDataIds.add(itemDayDataId);
                pushToMapArray(itemIds, log.address, event._itemId);
                break;
              }
              case CollectionV2ABI.events.SetApproved.topic:
                event = CollectionV2ABI.events.SetApproved.decode(log);
                break;
              case CollectionV2ABI.events.SetEditable.topic:
                event = CollectionV2ABI.events.SetEditable.decode(log);
                break;
              case CollectionV2ABI.events.Complete.topic:
                event = CollectionV2ABI.events.Complete.decode(log);
                break;
              case CollectionV2ABI.events.CreatorshipTransferred.topic:
                event =
                  CollectionV2ABI.events.CreatorshipTransferred.decode(log);
                break;
              case CollectionV2ABI.events.OwnershipTransferred.topic:
                event = CollectionV2ABI.events.OwnershipTransferred.decode(log);
                break;
              case CollectionV2ABI.events.Transfer.topic: {
                event = CollectionV2ABI.events.Transfer.decode(log);
                accountIds.add(event.to.toLowerCase());
                const timestamp = block.header.timestamp / 1000;
                const nftId = getNFTId(log.address, event.tokenId.toString());
                pushToMapArray(tokenIds, log.address, event.tokenId);
                transfers.set(
                  `${nftId}-${timestamp}`,
                  new Transfer({
                    id: `${nftId}-${timestamp}`,
                    nftId,
                    block: block.header.height,
                    from: event.from,
                    to: event.to,
                    network: ModelNetwork.POLYGON,
                    timestamp: BigInt(timestamp),
                    txHash: log.transactionHash,
                  })
                );
                break;
              }
            }
            if (event) {
              if (raritiesSnapshotDirty) {
                currentRaritiesSnapshot = new Map(
                  Array.from(rarities).map(([k, v]) => [k, { ...v } as Rarity])
                );
                raritiesSnapshotDirty = false;
              }
              collectionIds.add(log.address.toLowerCase());
              events.push({
                topic,
                event,
                block,
                log,
                transaction: log.transaction,
                rarities: currentRaritiesSnapshot,
                storeContractData: cachedStoreData,
              });
            } else {
              console.log("ERROR: Event not decoded correctly");
            }
            break;
          }
          case RaritiesABI.events.AddRarity.topic: {
            const event = RaritiesABI.events.AddRarity.decode(log);
            handleAddRarity(
              rarities,
              event,
              log.address === addresses.Rarity ? Currency.MANA : Currency.USD
            );
            raritiesSnapshotDirty = true;
            break;
          }
          case RaritiesABI.events.UpdatePrice.topic: {
            const event = RaritiesABI.events.UpdatePrice.decode(log);
            handleUpdatePrice(
              rarities,
              event,
              log.address === addresses.Rarity ? Currency.MANA : Currency.USD
            );
            raritiesSnapshotDirty = true;
            break;
          }
          case CollectionStoreABI.events.SetFee.topic: {
            const event = CollectionStoreABI.events.SetFee.decode(log);
            setStoreFee(event._newFee);
            break;
          }
          case CollectionStoreABI.events.SetFeeOwner.topic: {
            const event = CollectionStoreABI.events.SetFeeOwner.decode(log);
            setStoreFeeOwner(event._newFeeOwner);
            break;
          }
          case CollectionManagerABI.events.RaritiesSet.topic: {
            const event = CollectionManagerABI.events.RaritiesSet.decode(log);
            const rpcRarityStart = performance.now();
            await handleRaritiesSet(ctx, block.header, event, rarities);
            const rpcRarityDuration = performance.now() - rpcRarityStart;
            metrics.rpcCalls.rarity++;
            metrics.rpcTime.rarity += rpcRarityDuration;
            metrics.rpcTime.total += rpcRarityDuration;
            raritiesSnapshotDirty = true;
            break;
          }
          case OffChainMarketplaceABI.events.Traded.topic:
          case OffChainMarketplaceV3ABI.events.Traded.topic: {
            const event =
              topic === OffChainMarketplaceV3ABI.events.Traded.topic
                ? OffChainMarketplaceV3ABI.events.Traded.decode(log)
                : OffChainMarketplaceABI.events.Traded.decode(log);
            const tradeData = getTradeEventData(event, Network.MATIC);
            if (!tradeData) {
              break;
            }
            const { collectionAddress, buyer, seller, assetType, itemId } =
              tradeData;
            let tokenId = tradeData.tokenId;
            if (Number(assetType) === 4 && itemId !== undefined) {
              const collectionContract = new CollectionV2ABI.Contract(
                ctx,
                block.header,
                collectionAddress
              );
              const rpcItemsStart = performance.now();
              const item = await collectionContract.items(itemId);
              const rpcItemsDuration = performance.now() - rpcItemsStart;
              metrics.rpcCalls.items++;
              metrics.rpcTime.items += rpcItemsDuration;
              metrics.rpcTime.total += rpcItemsDuration;

              tokenId = encodeTokenId(Number(itemId), Number(item.totalSupply));
            }
            collectionIds.add(collectionAddress);

            if (tokenId) {
              pushToMapArray(tokenIds, collectionAddress, tokenId);
            } else {
              console.log("ERROR: tokenId not found in trade event data");
              break;
            }
            accountIds.add(seller);
            accountIds.add(buyer);
            analyticsIds.add(analyticDayDataId);
            const dayIDTrade = blockTimestamp / BigInt(86400);
            if (itemId !== undefined) {
              const itemIdStr = `${collectionAddress}-${itemId}`;
              const itemDayDataIdTrade = `${dayIDTrade.toString()}-${itemIdStr}`;
              itemDayDataIds.add(itemDayDataIdTrade);
            } else if (tokenId) {
              const nftIdTrade = `${collectionAddress}-${tokenId}`;
              const tempItemDayDataIdTrade = `${dayIDTrade.toString()}-nft-${nftIdTrade}`;
              itemDayDataIds.add(tempItemDayDataIdTrade);
            }

            if (raritiesSnapshotDirty) {
              currentRaritiesSnapshot = new Map(
                Array.from(rarities).map(([k, v]) => [k, { ...v } as Rarity])
              );
              raritiesSnapshotDirty = false;
            }
            events.push({
              topic,
              event,
              block,
              log,
              transaction: log.transaction,
              rarities: currentRaritiesSnapshot,
              storeContractData: cachedStoreData,
            });

            break;
          }
        }

        const eventDuration = performance.now() - eventStart;
        const eventType = topicToName[topic] || "other";
        eventTypeCounts[eventType] = (eventTypeCounts[eventType] || 0) + 1;
        eventTypeTimes[eventType] =
          (eventTypeTimes[eventType] || 0) + eventDuration;
      }
    }

    const accumulationLoopTime = performance.now() - accumulationLoopStart;

    const totalEventTime = Object.values(eventTypeTimes).reduce(
      (a, b) => a + b,
      0
    );
    const unexplainedTime =
      accumulationLoopTime - totalEventTime - preEventTime;

    const dbQueryStart = performance.now();
    const storedData = await getStoredData(ctx, {
      accountIds,
      collectionIds,
      tokenIds,
      analyticsIds,
      bidIds,
      itemIds,
      itemDayDataIds,
    });
    metrics.dbQueryTime = performance.now() - dbQueryStart;

    const { counts, accounts, orders, bids, nfts, items, metadatas } =
      storedData;

    const placeholderIds = [...itemDayDataIds].filter((id) =>
      id.includes("-nft-")
    );
    for (const placeholderId of placeholderIds) {
      const [dayID, , ...nftIdParts] = placeholderId.split("-");
      const nftId = nftIdParts.join("-");

      const nft = nfts.get(nftId);
      if (nft?.item) {
        itemDayDataIds.delete(placeholderId);
        const correctItemDayDataId = `${dayID}-${nft.item.id}`;
        itemDayDataIds.add(correctItemDayDataId);
      }
    }

    const additionalItemDayDatas = await ctx.store
      .findBy(ItemsDayData, {
        id: In([...Array.from(itemDayDataIds.values())]),
      })
      .then((q) => new Map(q.map((i) => [i.id, i])));

    for (const [key, value] of additionalItemDayDatas.entries()) {
      storedData.itemDayDatas.set(key, value);
    }

    const handleCollectionStart = performance.now();
    for (const {
      block,
      event,
      usedCredits,
      creditValue,
      txHash,
    } of collectionFactoryEvents) {
      const prefetched = prefetchedCollectionData.get(
        event._address.toLowerCase()
      );
      await handleCollectionCreation(
        ctx,
        block.header,
        event._address,
        storedData,
        usedCredits,
        creditValue,
        txHash,
        prefetched
      );
    }
    metrics.eventLoopBreakdown.proxyCreated =
      performance.now() - handleCollectionStart;

    const collectionEventsStart = performance.now();
    for (const {
      block,
      event,
      topic,
      log,
      transaction,
      rarities,
      storeContractData,
      bidV2ContractData,
      marketplaceContractData,
      marketplaceV2ContractData,
    } of events) {
      switch (topic) {
        case CollectionV2ABI.events.SetGlobalMinter.topic:
          handleSetGlobalMinter(
            log.address,
            event as CollectionV2ABI.SetGlobalMinterEventArgs,
            block.header,
            storedData
          );
          break;
        case CollectionV2ABI.events.SetGlobalManager.topic:
          handleSetGlobalManager(
            log.address,
            event as CollectionV2ABI.SetGlobalManagerEventArgs,
            storedData
          );
          break;
        case CollectionV2ABI.events.SetItemMinter.topic:
          handleSetItemMinter(
            log.address,
            event as CollectionV2ABI.SetItemMinterEventArgs,
            block.header,
            storedData
          );
          break;
        case CollectionV2ABI.events.SetItemManager.topic:
          handleSetItemManager(
            log.address,
            event as CollectionV2ABI.SetItemManagerEventArgs,
            storedData
          );
          break;
        case CollectionV2ABI.events.AddItem.topic:
          rarities &&
            (await handleAddItem(
              ctx,
              block.header,
              log.address,
              event as CollectionV2ABI.AddItemEventArgs,
              storedData,
              rarities
            ));
          break;
        case CollectionV2ABI.events.RescueItem.topic:
          transaction &&
            handleRescueItem(
              event as CollectionV2ABI.RescueItemEventArgs,
              block.header,
              log,
              transaction,
              storedData,
              inMemoryData
            );
          break;
        case CollectionV2ABI.events.UpdateItemData.topic:
          handleUpdateItemData(
            log.address,
            event as CollectionV2ABI.UpdateItemDataEventArgs,
            block.header,
            storedData
          );
          break;
        case OffChainMarketplaceABI.events.Traded.topic:
        case OffChainMarketplaceV3ABI.events.Traded.topic: {
          if (!storeContractData || !transaction) {
            console.log("ERROR: storeContractData not found");
            break;
          }
          await handleTraded(
            ctx,
            event as OffChainMarketplaceABI.TradedEventArgs,
            block,
            transaction,
            storedData,
            inMemoryData
          );
          break;
        }
        case CollectionV2ABI.events.Issue.topic:
          if (!storeContractData) {
            console.log("ERROR: storeContractData not found");
            break;
          }
          if (
            (event as CollectionV2ABI.IssueEventArgs)._caller ===
              addresses.OffChainMarketplace ||
            (event as CollectionV2ABI.IssueEventArgs)._caller ===
              addresses.OffChainMarketplaceV2 ||
            (event as CollectionV2ABI.IssueEventArgs)._caller ===
              addresses.OffChainMarketplaceV3
          ) {
            break;
          }
          await handleIssue(
            ctx,
            log.address,
            event as CollectionV2ABI.IssueEventArgs,
            block.header,
            transaction!,
            storedData,
            inMemoryData,
            storeContractData
          );
          break;
        case CollectionV2ABI.events.SetApproved.topic:
          !!transaction &&
            handleSetApproved(
              log.address,
              event as CollectionV2ABI.SetApprovedEventArgs,
              block.header,
              log,
              transaction,
              storedData
            );
          break;
        case CollectionV2ABI.events.SetEditable.topic:
          handleSetEditable(
            log.address,
            event as CollectionV2ABI.SetEditableEventArgs,
            storedData
          );
          break;
        case CollectionV2ABI.events.Complete.topic:
          handleCompleteCollection(log.address, storedData);
          break;
        case CollectionV2ABI.events.CreatorshipTransferred.topic:
          handleTransferCreatorship(
            log.address,
            event as CollectionV2ABI.CreatorshipTransferredEventArgs,
            block.header,
            storedData
          );
          break;
        case CollectionV2ABI.events.OwnershipTransferred.topic:
          handleTransferOwnership(
            log.address,
            event as CollectionV2ABI.OwnershipTransferredEventArgs,
            block.header,
            storedData
          );
          break;
        case CollectionV2ABI.events.Transfer.topic:
          try {
            await handleTransfer(
              ctx,
              log.address,
              event as CollectionV2ABI.TransferEventArgs,
              block.header,
              storedData,
              inMemoryData,
              log.transactionHash
            );
          } catch (e) {
            console.log("Error in handleTransfer:", e);
            console.log("Transfer event failed for NFT:", log.address, event);
          }
          break;

        case MarketplaceABI.events.OrderCreated.topic: {
          handleOrderCreated(
            event as MarketplaceABI.OrderCreatedEventArgs,
            block,
            log.address,
            log.transactionHash,
            orders,
            nfts,
            counts
          );
          break;
        }
        case MarketplaceABI.events.OrderSuccessful.topic: {
          if (!marketplaceContractData || !marketplaceV2ContractData) {
            console.log(
              "ERROR: marketplaceContractData or marketplaceV2ContractData not found"
            );
            break;
          }
          await handleOrderSuccessful(
            ctx,
            event as MarketplaceABI.OrderSuccessfulEventArgs,
            block,
            log.transactionHash,
            marketplaceContractData,
            marketplaceV2ContractData,
            storedData,
            inMemoryData
          );
          break;
        }
        case MarketplaceABI.events.OrderCancelled.topic: {
          handleOrderCancelled(
            event as MarketplaceABI.OrderCancelledEventArgs,
            block,
            nfts,
            orders
          );
          break;
        }
        case ERC721BidABI.events.BidCreated.topic: {
          handleBidCreated(
            event as ERC721BidABI.BidCreatedEventArgs,
            block,
            log.address,
            nfts,
            bids,
            counts
          );
          break;
        }
        case ERC721BidABI.events.BidAccepted.topic: {
          if (!bidV2ContractData) {
            console.log("ERROR: bidV2ContractData not found");
            break;
          }
          await handleBidAccepted(
            ctx,
            event as ERC721BidABI.BidAcceptedEventArgs,
            block,
            log.transactionHash,
            bidV2ContractData,
            storedData,
            inMemoryData
          );
          break;
        }
        case ERC721BidABI.events.BidCancelled.topic: {
          handleBidCancelled(
            event as ERC721BidABI.BidCancelledEventArgs,
            block,
            bids,
            nfts
          );
          break;
        }
      }
    }
    metrics.eventLoopBreakdown.collectionEvents =
      performance.now() - collectionEventsStart;

    for (const event of committeeEvents) {
      handleMemeberSet(accounts, event);
    }

    metrics.eventLoopTime =
      accumulationLoopTime +
      metrics.rpcTime.total +
      metrics.eventLoopBreakdown.proxyCreated +
      metrics.eventLoopBreakdown.collectionEvents;

    if (metrics.eventLoopTime > 1000 || metrics.dbQueryTime > 1000) {
      const topEvents = Object.entries(eventTypeTimes)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 5)
        .map(([type, time]) => `${type}=${fmt(time)}(${eventTypeCounts[type]})`)
        .join(", ");

      console.log(
        `\u{1F4CD} Breakdown: accumulation=${fmt(accumulationLoopTime)}, RPC=${fmt(
          metrics.rpcTime.total
        )}, handleCollection=${fmt(
          metrics.eventLoopBreakdown.proxyCreated
        )}, processEvents=${fmt(metrics.eventLoopBreakdown.collectionEvents)}`
      );
      console.log(`   \u{2514}\u{2500} Events: ${topEvents}`);
      if (skippedTransfers > 0 || processedTransfers > 0) {
        const skipPct = (
          (skippedTransfers / (skippedTransfers + processedTransfers)) *
          100
        ).toFixed(1);
        console.log(
          `   \u{2514}\u{2500} Transfers: ${processedTransfers} processed, ${skippedTransfers} skipped (${skipPct}% filtered out)`
        );
      }
    }

    let squidRouterOrdersCreated = 0;
    for (const [txKey, orderHash] of orderHashByTx.entries()) {
      console.log(
        `Creating SquidRouterOrder for tx ${txKey} and order hash ${orderHash}`
      );
      const creditEvents = creditEventsByTx.get(txKey);

      if (creditEvents && creditEvents.length > 0) {
        const totalCreditValue = creditEvents.reduce(
          (sum, c) => sum + c.value,
          BigInt(0)
        );

        const [blockHeightStr] = txKey.split("-");
        const blockHeight = parseInt(blockHeightStr, 10);

        const block = ctx.blocks.find((b) => b.header.height === blockHeight);
        if (!block) {
          console.log(
            `\u{26A0}\u{FE0F} [SquidRouter] Could not find block ${blockHeight} for order ${orderHash}`
          );
          continue;
        }

        const txIndex = parseInt(txKey.split("-")[1], 10);
        const logInTx = block.logs.find((l) => l.transactionIndex === txIndex);
        const txHash = logInTx?.transactionHash || "unknown";

        const squidRouterOrder = new SquidRouterOrder({
          id: orderHash,
          orderHash,
          creditIds: creditEvents.map((c) => c.creditId),
          totalCreditsUsed: totalCreditValue,
          txHash,
          blockNumber: BigInt(blockHeight),
          timestamp: BigInt(block.header.timestamp / 1000),
          network: ModelNetwork.POLYGON,
        });

        squidRouterOrders.set(orderHash, squidRouterOrder);
        squidRouterOrdersCreated++;

        console.log(
          `\u{2705} [SquidRouter] Created order: hash=${orderHash.slice(
            0,
            18
          )}..., credits=${
            creditEvents.length
          }, value=${totalCreditValue}, txHash=${txHash.slice(0, 18)}...`
        );
      }
    }

    if (squidRouterOrdersCreated > 0) {
      console.log(
        `\u{1F4E6} [SquidRouter] Created ${squidRouterOrdersCreated} SquidRouterOrders in this batch`
      );
    }

    const upsertResult = await performUpserts(
      ctx.store,
      fmt,
      storedData,
      rarities,
      metadatas,
      items,
      nfts,
      orders,
      bids,
      sales,
      mints,
      transfers,
      curations,
      squidRouterOrders
    );

    metrics.upsertTime = upsertResult.timing.total;

    if (transferGiftCandidates.size > 0) {
      const soldKeys = new Set<string>();
      for (const sale of sales.values()) {
        soldKeys.add(`${sale.txHash.toLowerCase()}-${sale.nft.id}`);
      }

      const floor =
        batchLastNotified && batchLastNotified > minTransferNotificationTimestamp
          ? batchLastNotified
          : minTransferNotificationTimestamp;

      let maxNotified = floor;
      let emitted = 0;
      for (const candidate of transferGiftCandidates.values()) {
        if (candidate.timestamp <= floor) {
          continue;
        }
        if (
          soldKeys.has(`${candidate.txHash.toLowerCase()}-${candidate.nftId}`)
        ) {
          continue;
        }
        try {
          await publishTransferGift(candidate);
          emitted++;
          if (candidate.timestamp > maxNotified) {
            maxNotified = candidate.timestamp;
          }
        } catch (e) {
          console.log(
            "Error publishing transfer gift for NFT",
            candidate.nftId,
            e
          );
        }
      }

      if (emitted > 0 && maxNotified > (batchLastNotified ?? 0n)) {
        await setLastNotified(ctx.store, maxNotified);
      }
    }

    const totalBatchTime = performance.now() - batchStartTime;

    const pctDb =
      totalBatchTime > 0
        ? ((metrics.dbQueryTime / totalBatchTime) * 100).toFixed(1)
        : "0";
    const pctEvent =
      totalBatchTime > 0
        ? ((metrics.eventLoopTime / totalBatchTime) * 100).toFixed(1)
        : "0";
    const pctUpsert =
      totalBatchTime > 0
        ? ((metrics.upsertTime / totalBatchTime) * 100).toFixed(1)
        : "0";

    const warnings: string[] = [];
    if (metrics.dbQueryTime > 1000)
      warnings.push(`DB Queries: ${fmt(metrics.dbQueryTime)}`);
    if (metrics.eventLoopTime > 1000)
      warnings.push(`Event Loop: ${fmt(metrics.eventLoopTime)}`);
    if (metrics.rpcTime.total > 1000)
      warnings.push(`RPC: ${fmt(metrics.rpcTime.total)}`);
    if (metrics.upsertTime > 1000)
      warnings.push(`DB Upserts: ${fmt(metrics.upsertTime)}`);
    if (totalBatchTime > 3000) warnings.push(`Total: ${fmt(totalBatchTime)}`);

    const warningLine =
      warnings.length > 0 ? `\n\u{26A0}\u{FE0F}  WARNING SLOW: ${warnings.join(" | ")}` : "";

    const rpcBreakdown =
      metrics.rpcTime.total > 0
        ? `\n\u{1F310} RPC: ${fmt(metrics.rpcTime.total)} (owner: ${fmt(
            metrics.ownerMulticallTime
          )}, items: ${fmt(metrics.rpcTime.items)}, rarity: ${fmt(
            metrics.rpcTime.rarity
          )})`
        : "";

    totalEventsProcessed += metrics.eventsProcessed;

    if (process.env.BATCH_METRICS_LOGS === "true" || totalBatchTime > 3000) {
      console.log(`
\u{1F4CA} ============ POLYGON BATCH METRICS ============
\u{1F4E6} Blocks: ${metrics.blockRange}
\u{23F1}\u{FE0F}  Total: ${fmt(totalBatchTime)}
   \u{251C}\u{2500} DB Queries: ${fmt(metrics.dbQueryTime)} (${pctDb}%)
   \u{251C}\u{2500} Event Loop: ${fmt(metrics.eventLoopTime)} (${pctEvent}%)
   \u{2514}\u{2500} DB Upserts: ${fmt(metrics.upsertTime)} (${pctUpsert}%)
\u{1F4C8} Events: ${
        metrics.eventsProcessed
      } (total: ${totalEventsProcessed.toLocaleString()})
\u{1F517} RPC: owner=${metrics.rpcCalls.owner}, items=${
        metrics.rpcCalls.items
      }, rarity=${metrics.rpcCalls.rarity}${rpcBreakdown}
\u{1F4BE} Entities: NFTs=${nfts.size}, Items=${items.size}, Collections=${
        storedData.collections.size
      }, Orders=${orders.size}, Sales=${sales.size}, Bids=${bids.size}, Mints=${
        mints.size
      }, Transfers=${transfers.size}, Curations=${curations.size}${warningLine}
=================================================
`);
    }

    ctx.log.info(
      `Batch ${metrics.blockRange} saved: nfts=${nfts.size}, items=${items.size}, sales=${sales.size}, mints=${mints.size}, transfers=${transfers.size}`
    );

  },
  { prometheus }
);
