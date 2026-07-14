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
import * as MarketplaceV3ABI from "./abi/DecentralandMarketplacePolygon";
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
  setMarketplaceV3FeeCollector,
  setMarketplaceV3FeeRate,
  setMarketplaceV3RoyaltiesRate,
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
} from "../common/utils/marketplaceV3";
import {
  getLastNotified,
  setLastNotified,
  publishTransferGift,
} from "../common/utils/events";
import {
  recordIndexingStart,
  notifyHeadReachedOnce,
} from "../common/utils/head-notification";

const schemaName = process.env.DB_SCHEMA;
const addresses = getAddresses(Network.MATIC);
let bytesRead = 0;

// Per-batch invariants, computed once at module load instead of every batch.
// Lowercased address sets for O(1) lookups inside the event loop.
const creditsManagerAddresses = new Set(
  addresses.CreditsManager.map((a: string) => a.toLowerCase())
);
const collectionFactoryAddresses = new Set(
  [addresses.CollectionFactory, addresses.CollectionFactoryV3].map((c) =>
    c.toLowerCase()
  )
);
const spokeAddressLower = addresses.Spoke?.toLowerCase();

// Format a duration in ms (show seconds if > 1000ms).
const fmt = (ms: number) =>
  ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms)}ms`;

// Push to a Map<K, V[]> without the O(n2) spread of `[...(map.get(key) ?? []), v]`.
function pushToMapArray<K, V>(map: Map<K, V[]>, key: K, value: V): void {
  let arr = map.get(key);
  if (!arr) {
    arr = [];
    map.set(key, arr);
  }
  arr.push(value);
}

// Topic hash -> human-readable event name, computed once (not per event).
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
  [MarketplaceV3ABI.events.Traded.topic]: "Traded",
};
const preloadedCollections = loadCollections().addresses;
// Set form for O(1) membership checks in the per-log hot path (~84k logs/batch).
const preloadedCollectionsSet = new Set(preloadedCollections);
const preloadedCollectionsHeight = loadCollections().height;
// Cache lastNotified timestamp to avoid querying DB for historical blocks
let cachedLastNotified: bigint | null = null;
let lastNotifiedLoaded = false;
// Hard lower bound (epoch seconds) for gift notifications. Unlike last_notified
// (which lives in public.squids and could be stale), this floor comes from the
// environment, so it survives any DB wipe / re-index and guarantees we never
// backfill historical gift notifications. Defaults to process start time when
// unset, which is also safe (a re-index/restart never replays old gifts).
const minTransferNotificationTimestamp = BigInt(
  process.env.MIN_TRANSFER_NOTIFICATION_TIMESTAMP ??
    Math.floor(Date.now() / 1000)
);

//  BULK INDEX MODE: Drop indices during initial sync, recreate when caught up.
// Opt-in and default off; enable via env var BULK_INDEX_MODE=true.
const BULK_INDEX_MODE = process.env.BULK_INDEX_MODE === "true";
let bulkModeInitialized = false;
let indicesRecreated = false;
let indicesNeedRecreation = false;
// recreateIndices throws while anything is still missing, so the head handler retries it. Bounded
// because at head a batch runs every few seconds: an index that can never be built (a UNIQUE one
// with duplicate rows, say) would otherwise re-attempt forever.
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

  // PHASE 1: independent entities. Sequential on purpose -- every store call goes
  // through the single Postgres connection of the batch transaction, so a
  // Promise.all here only queues the queries, it does not parallelize them.
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

  // PHASE 2: Metadatas -> Items (items reference metadata)
  t0 = performance.now();
  await store.upsert([...metadatas.values()]);
  timing.metadatas = performance.now() - t0;

  t0 = performance.now();
  await store.upsert([...items.values()]);
  timing.items = performance.now() - t0;

  // PHASE 3: NFT <-> Order circular dependency workaround
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

  // Restore activeOrder and collect NFTs that need update
  const nftsWithOrders: NFT[] = [];
  for (const [nftId, order] of orderByNFT) {
    const nft = nfts.get(nftId);
    if (nft) {
      nft.activeOrder = order;
      nftsWithOrders.push(nft);
    }
  }

  // PHASE 4: NFTs with orders + bids + inserts (sequential -- same connection).
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
  // Portal ingests from the finalized stream; a log-filtered stream yields
  // non-contiguous blocks which the hot-block path rejects. Polygon finality is
  // only a few blocks (~seconds) behind head, so this stays effectively real-time.
  supportHotBlocks: false,
  stateSchema: `polygon_processor_${schemaName}`,
});
// Expose Prometheus metrics (sqd_processor_last_block / chain_height) -- the squid
// management server scrapes /metrics on this port to detect a live processor.
// setGateway used to start this; with the Portal run() we wire it explicitly.
const prometheus = new PrometheusServer();
prometheus.setPort(Number(process.env.POLYGON_PROMETHEUS_PORT || 3001));
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
          // BigInt-safe: Portal blocks can carry bigint fields that JSON.stringify
          // rejects; this is only a byte-count metric, so serialize them as strings.
          JSON.stringify(block, (_k, v) =>
            typeof v === "bigint" ? v.toString() : v
          ),
          "utf8"
        ),
      0
    );

    // Track indexing progress and alert Slack the first time this indexer reaches
    // head. Done before any early-return below, since head can be reached on a
    // batch with no DCL-relevant data.
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

        // Fresh sync (new deploy near the initial block) vs restart of an already
        // synced squid, using INITIAL_BLOCK from config with a 10% threshold.
        const freshSync = isFreshSync(currentBlock, INITIAL_BLOCK);
        indicesNeedRecreation = await checkIndicesNeedRecreation(
          ctx.store,
          POLYGON_INDICES
        );

        console.log(
          `[IndexMgr] Decision: block=${currentBlock.toLocaleString()}, freshSync=${freshSync}, indicesNeedRecreation=${indicesNeedRecreation}, isHead=${ctx.isHead}`
        );

        if (freshSync) {
          // New deploy: indices exist from migrations; drop them for faster loading.
          console.log(`[IndexMgr] Fresh sync - dropping indices for bulk indexing`);
          await dropIndicesForBulkLoad(ctx.store, POLYGON_INDICES);
          indicesNeedRecreation = true;
        } else if (!indicesNeedRecreation) {
          // Restart of an already synced squid: all indices exist, leave them.
          console.log(`[IndexMgr] Restart of synced squid - all indices present`);
          indicesRecreated = true;
        } else if (ctx.isHead) {
          // At head but missing indices - recreate them now.
          console.log(`[IndexMgr] At head with missing indices - recreating now`);
          await recreateIndices(ctx.store, POLYGON_INDICES);
          indicesRecreated = true;
        } else {
          // Mid-sync restart with missing indices - recreate when we reach head.
          console.log(`[IndexMgr] Mid-sync restart - will recreate indices at head`);
        }
      } catch (e: any) {
        // Never throw - index management must not break indexing.
        console.log(`[IndexMgr] Error in bulk index mode init: ${e.message}`);
      }
    }

    //  BULK INDEX MODE: recreate indices once we reach head. This MUST run before
    // this batch reads or writes any managed table: recreateIndices issues plain
    // CREATE INDEX (SHARE lock) on an independent connection, and if the batch
    // transaction had already taken ROW EXCLUSIVE locks (via performUpserts) the two
    // connections would deadlock -- the batch tx can't commit until the handler
    // returns, and the handler is awaiting the CREATE INDEX. Running it here, before
    // any table access, keeps the connections contention-free. recreateIndices is a
    // no-op when nothing is missing; on error we retry next batch.
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
          log.address === addresses.MarketplaceV3 ||
          log.address === addresses.MarketplaceV3_V2 ||
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

    // Load lastNotified once at startup to compare with batch timestamps
    // This avoids querying DB for every transfer in historical blocks
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

    // If processing new blocks, reload lastNotified once per batch to keep it updated
    // If processing historical blocks, pass null to skip sending events
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

    // Index: blockHeight-txIndex -> CreditUsed events
    const creditEventsByTx = new Map<
      string,
      { creditId: string; value: bigint }[]
    >();
    // Index: blockHeight-txIndex -> OrderCreated orderHash
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

        // 2. Index Spoke OrderCreated events (for cross-chain operations like NAME registration)
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

    //  OPTIMIZATION: Fetch ALL collection data via MULTICALL (9 calls per collection -> 1 batch)
    // This is the biggest optimization: instead of 9 RPC calls per collection,
    // we fetch name, symbol, owner, creator, isCompleted, isApproved, isEditable, baseURI, chainId
    // for ALL collections in a single multicall batch!
    let prefetchedCollectionData = new Map<string, CollectionData>();

    if (proxyCreatedEvents.length > 0) {
      const multicallStart = performance.now();

      // Use the LAST block in the batch for multicall (all collections exist by then)
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

    //  OPTIMIZATION: Create rarities snapshot ONCE, only update when rarities change
    // This reduces from O(n*m) to O(k*m) where n=events, m=rarities, k=rarity-changing events
    let currentRaritiesSnapshot: Map<string, Rarity> = new Map(
      Array.from(rarities).map(([k, v]) => [k, { ...v } as Rarity])
    );
    let raritiesSnapshotDirty = false;

    //  OPTIMIZATION: Pre-compute valid collections Set ONCE for O(1) lookup
    const validCollections = new Set<string>([
      ...preloadedCollections,
      ...collectionIdsNotIncludedInPreloaded,
    ]);
    // collectionIdsCreatedInBatch is added dynamically during the loop

    for (let block of ctx.blocks) {
      const blockTimestamp = BigInt(block.header.timestamp / 1000);
      const dayId = (blockTimestamp / BigInt(86400)).toString();

      for (let log of block.logs) {
        const topic = log.topics[0];

        //  FAST PATH: Skip non-DCL Transfer events BEFORE any other processing
        // This avoids performance.now() calls, switch overhead, etc for 99%+ of events
        // Transfer is the most common event in blockchain - we get 84k+ per batch from ALL contracts
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

            // Lowercase to match the fast-path lookup: log.address (and the DB /
            // preloaded ids seeded into validCollections) are lowercase, but the
            // decoded event._address may be checksummed. Without this, Transfers of
            // a collection created earlier in the same batch would be skipped.
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
            // Note: Collection creations with credits are NOT cross-chain
            // SquidRouterOrders are created separately for cross-chain operations
            // (e.g., NAME registration that bridges from Polygon to Ethereum)
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
          // Keep the cached V3 fee configuration current. Same shape as the V1/V2 cases below,
          // and it inherits their one caveat: these are applied while events are accumulated,
          // whereas Traded is handled later in the batch. So a fee change and trades in the SAME
          // batch are applied out of order -- trades before the change would see the new value.
          // At head a batch is seconds wide so this cannot happen; during a backfill a batch can
          // span ~1M blocks, and it would only matter around the handful of blocks where fees
          // actually changed. Resolving per-trade needs the change recorded with its block and
          // applied as-of, which is a bigger change than this one.
          case MarketplaceV3ABI.events.FeeCollectorUpdated.topic: {
            setMarketplaceV3FeeCollector(
              MarketplaceV3ABI.events.FeeCollectorUpdated.decode(log)._feeCollector
            );
            break;
          }
          case MarketplaceV3ABI.events.FeeRateUpdated.topic: {
            setMarketplaceV3FeeRate(
              MarketplaceV3ABI.events.FeeRateUpdated.decode(log)._feeRate
            );
            break;
          }
          case MarketplaceV3ABI.events.RoyaltiesRateUpdated.topic: {
            setMarketplaceV3RoyaltiesRate(
              MarketplaceV3ABI.events.RoyaltiesRateUpdated.decode(log)._royaltiesRate
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
            //  NOTE: Non-DCL Transfers are filtered BEFORE the switch (fast path above)
            // If we get here, this IS a valid DCL collection transfer
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
                //  OPTIMIZATION: Use pushToMapArray() to avoid O(n2) spread
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
              //  OPTIMIZATION: Only refresh snapshot if rarities changed since last snapshot
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
            // ! RPC CALLS: handleRaritiesSet makes multiple RPC calls (raritiesCount + rarities[i])
            const rpcRarityStart = performance.now();
            await handleRaritiesSet(ctx, block.header, event, rarities);
            const rpcRarityDuration = performance.now() - rpcRarityStart;
            metrics.rpcCalls.rarity++;
            metrics.rpcTime.rarity += rpcRarityDuration;
            metrics.rpcTime.total += rpcRarityDuration;
            raritiesSnapshotDirty = true;
            break;
          }
          case MarketplaceV3ABI.events.Traded.topic: {
            const event = MarketplaceV3ABI.events.Traded.decode(log);
            const tradeData = getTradeEventData(event, Network.MATIC);
            // Nothing to index: not an order or a bid (a giveaway has no payment leg).
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
              // ! RPC CALL: collectionContract.items() - one per Traded secondary sale
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
              // Secondary sale - add placeholder to be resolved later
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

    // Resolve placeholder itemDayDataIds for secondary sales
    const placeholderIds = [...itemDayDataIds].filter((id) =>
      id.includes("-nft-")
    );
    for (const placeholderId of placeholderIds) {
      // Extract dayID and nftId from placeholder: "dayID-nft-contractAddress-tokenId"
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

    // Processed sequentially: every call read-modify-writes shared storedData
    // (counts/collections/accounts) across awaits, so running them in parallel
    // races on that state. With multicall data prefetched there is no I/O left to
    // overlap, so a for...of is both correct and just as fast.
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
        case MarketplaceV3ABI.events.Traded.topic: {
          if (!storeContractData || !transaction) {
            console.log("ERROR: storeContractData not found");
            break;
          }
          await handleTraded(
            ctx,
            event as MarketplaceV3ABI.TradedEventArgs,
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
              addresses.MarketplaceV3 ||
            (event as CollectionV2ABI.IssueEventArgs)._caller ===
              addresses.MarketplaceV3_V2
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
            // Continue processing other events even if this one fails
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

    //  CREATE SQUID ROUTER ORDERS for cross-chain operations
    // These are created when there's a CreditUsed + Spoke.OrderCreated in the same transaction
    // This is for cross-chain operations like NAME registration (not collection creation)
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

        // Parse txKey to get block and tx info (format: "blockHeight-txIndex")
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

    // --- Gift notifications (TRANSFER_RECEIVED) ---
    // A transfer is a genuine gift only if it is NOT part of a marketplace
    // operation. Sales (orders, bids and offchain trades) all create a Sale via
    // trackSale in this same batch, keyed by (txHash, nft). Any transfer whose
    // (txHash, nftId) matches a Sale is a purchase and must not notify the buyer
    // as a gift. We decide this here, post-batch, because within a transaction
    // the ERC721 Transfer log is processed before the marketplace event that
    // records the sale.
    if (transferGiftCandidates.size > 0) {
      const soldKeys = new Set<string>();
      for (const sale of sales.values()) {
        soldKeys.add(`${sale.txHash.toLowerCase()}-${sale.nft.id}`);
      }

      // Never (re-)emit at or below this floor. The env-based floor protects
      // against backfilling on a re-index even if last_notified is stale; the
      // watermark dedupes incrementally at head.
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
          // It's a marketplace purchase, not a gift.
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

    // Detailed per-batch metrics are verbose (~18 lines/batch). Only emit them when
    // explicitly enabled or when the batch was slow (>3s), to avoid log spam.
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
