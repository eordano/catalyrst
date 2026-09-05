/**
 * Index Manager - Drop/Recreate indices for faster bulk indexing
 *
 * During initial sync, maintaining secondary indices is expensive. We drop the
 * non-essential ones and recreate them when the processor catches up with the
 * chain head. Only secondary indices used by the query layer are managed here --
 * never primary keys, unique constraints or FK-supporting indices.
 *
 * Indices are split per network (POLYGON_INDICES / ETH_INDICES) so each processor
 * manages only the tables it owns. Tables that both processors write (nft, order,
 * sale, bid, transfer, metadata, account, wearable) are managed by the Polygon
 * processor, which is by far the dominant writer of those rows; the ETH processor
 * only manages its exclusive tables (parcel, estate, ens, data).
 *
 * MOST query-layer indices are not needed by either processor's own path, which looks entities up
 * by primary key -- so dropping the shared ones during Polygon's backfill does not slow ETH's.
 * Two are the exception: getStoredData resolves orders by `nft_id` and items by `collection_id`,
 * neither of which is a primary key. Those carry `keepDuringBulkLoad` and are never dropped.
 * Assuming the write path was PK-only cost a 4.6x slowdown on a prod backfill before it was
 * caught, so verify against the actual store queries before adding an index here.
 *
 * All DDL runs on an independent connection, outside the batch transaction (see
 * withIndexConnection), so each CREATE/DROP INDEX autocommits on its own instead
 * of being held inside the multi-hour processor transaction. Callers must invoke
 * recreateIndices BEFORE their batch writes to any managed table (see the head
 * handlers): a plain CREATE INDEX takes a SHARE lock that conflicts with the ROW
 * EXCLUSIVE locks a writing batch transaction holds, and since that transaction
 * cannot commit until its handler returns, running the DDL after the writes would
 * self-deadlock on the same processor. A note on shared tables: when the Polygon
 * processor recreates a shared-table index (nft/order/sale/...) at head, a
 * concurrent ETH batch's upserts can briefly block on the same SHARE<->ROW EXCLUSIVE
 * conflict -- not a deadlock (it clears when the ETH batch commits), and it happens
 * once, so the practical impact is negligible.
 *
 * All logs use prefix [IndexMgr] for easy filtering in Grafana.
 */

import { Store } from "@subsquid/typeorm-store";
import { EntityManager, QueryRunner } from "typeorm";
import { createSlackComponent, ISlackComponent } from "./slack";

// Log prefix for easy filtering in Grafana
const LOG_PREFIX = "[IndexMgr]";

/**
 * `keepDuringBulkLoad` marks an index the PROCESSOR's own read path needs, not just the query
 * layer. Bulk mode leaves those in place: dropping them makes the backfill slower, because every
 * batch then seq-scans the table they serve. See the two flagged below.
 */
export type ManagedIndex = {
  name: string;
  create: string;
  keepDuringBulkLoad?: boolean;
};
export type IndexGroup = Record<string, ManagedIndex[]>;

// Secondary indices owned by the Polygon processor (plus the shared marketplace
// tables -- see file header). Do NOT include PKs, UNIQUE/REL constraints or any
// index a constraint depends on. Grouped by table, heaviest first.
export const POLYGON_INDICES: IndexGroup = {
  nft: [
    { name: "IDX_5f8cc4778564d0bd3c4ac3436d", create: `CREATE INDEX "IDX_5f8cc4778564d0bd3c4ac3436d" ON "nft" ("search_order_status", "search_order_expires_at", "category")` },
    { name: "IDX_d5b8837a62eb6d9c95eb3d2ef2", create: `CREATE INDEX "IDX_d5b8837a62eb6d9c95eb3d2ef2" ON "nft" ("search_order_status", "search_order_expires_at", "network")` },
    { name: "IDX_26e756121a20d1cc3e4d738279", create: `CREATE INDEX "IDX_26e756121a20d1cc3e4d738279" ON "nft" ("owner_address")` },
    { name: "IDX_0fca1a8c5d9399d9a9a52e26f7", create: `CREATE INDEX "IDX_0fca1a8c5d9399d9a9a52e26f7" ON "nft" ("contract_address", "token_id")` },
    { name: "IDX_3baa214ec3db0ce29708750e3b", create: `CREATE INDEX "IDX_3baa214ec3db0ce29708750e3b" ON "nft" ("category")` },
    { name: "IDX_e0e405184c1c9253bbe95b6cc7", create: `CREATE INDEX "IDX_e0e405184c1c9253bbe95b6cc7" ON "nft" ("search_order_expires_at_normalized")` },
    { name: "IDX_b53fdf02d6f6047c1758ae885a", create: `CREATE INDEX "IDX_b53fdf02d6f6047c1758ae885a" ON "nft" ("search_is_land")` },
    { name: "IDX_4c7d1118621f3ea97740a1d876", create: `CREATE INDEX "IDX_4c7d1118621f3ea97740a1d876" ON "nft" ("item_id", "owner_id")` },
    { name: "IDX_2c8ca873555fc156848199919f", create: `CREATE INDEX "IDX_2c8ca873555fc156848199919f" ON "nft" ("created_at")` },
    { name: "IDX_645ec1a1710c449fa4e9d241e9", create: `CREATE INDEX "IDX_645ec1a1710c449fa4e9d241e9" ON "nft" ("search_order_expires_at")` },
    { name: "IDX_4d213d73326e54427a5c9bdddf", create: `CREATE INDEX "IDX_4d213d73326e54427a5c9bdddf" ON "nft" ("search_parcel_is_in_bounds")` },
    { name: "IDX_7e215df412b248db3731737290", create: `CREATE INDEX "IDX_7e215df412b248db3731737290" ON "nft" ("token_id")` },
    { name: "IDX_ffe58aa05707db77c2f20ecdbc", create: `CREATE INDEX "IDX_ffe58aa05707db77c2f20ecdbc" ON "nft" ("collection_id")` },
    { name: "IDX_c36d2ea36d7de5e265c30b8be8", create: `CREATE INDEX "IDX_c36d2ea36d7de5e265c30b8be8" ON "nft" ("metadata_id")` },
    { name: "IDX_83cfd3a290ed70c660f8c9dfe2", create: `CREATE INDEX "IDX_83cfd3a290ed70c660f8c9dfe2" ON "nft" ("owner_id")` },
    { name: "IDX_b92ac830e4b3a630162a898203", create: `CREATE INDEX "IDX_b92ac830e4b3a630162a898203" ON "nft" ("active_order_id")` },
  ],
  order: [
    { name: "IDX_2485593ed8c9972197aeaf7da6", create: `CREATE INDEX "IDX_2485593ed8c9972197aeaf7da6" ON "order" ("expires_at_normalized")` },
    { name: "IDX_d01158fe15b1ead5c26fd7f4e9", create: `CREATE INDEX "IDX_d01158fe15b1ead5c26fd7f4e9" ON "order" ("item_id")` },
    // KEPT during bulk load: getStoredData does `findBy(Order, { nft: In([...nftIds]) })` once per
    // batch. Without this index that is a seq scan of `order` per batch, and it gets worse as the
    // table grows -- measured at 4.6x slower overall on a prod backfill.
    { name: "IDX_f5047ff046d513a3598c1a2931", create: `CREATE INDEX "IDX_f5047ff046d513a3598c1a2931" ON "order" ("nft_id")`, keepDuringBulkLoad: true },
  ],
  sale: [
    { name: "IDX_8ac00a610840894296c6f32fd2", create: `CREATE INDEX "IDX_8ac00a610840894296c6f32fd2" ON "sale" ("timestamp")` },
    { name: "IDX_a91d7a7aa55af7d57ef4d17912", create: `CREATE INDEX "IDX_a91d7a7aa55af7d57ef4d17912" ON "sale" ("search_category", "network")` },
    { name: "IDX_439a57a4a0d130329d3d2e671b", create: `CREATE INDEX "IDX_439a57a4a0d130329d3d2e671b" ON "sale" ("item_id")` },
    { name: "IDX_8524438f82167bcb795bcb8663", create: `CREATE INDEX "IDX_8524438f82167bcb795bcb8663" ON "sale" ("nft_id")` },
  ],
  item: [
    // KEPT during bulk load: getStoredData looks items up by `collection` (not by PK) once per
    // batch, for the same reason as order.nft_id above.
    { name: "IDX_9ddbd0267ddb9c59621775f94e", create: `CREATE INDEX "IDX_9ddbd0267ddb9c59621775f94e" ON "item" ("collection_id", "blockchain_id")`, keepDuringBulkLoad: true },
    { name: "IDX_6d5bb320c601281cd3a213979e", create: `CREATE INDEX "IDX_6d5bb320c601281cd3a213979e" ON "item" ("metadata_id")` },
  ],
  bid: [
    { name: "IDX_3caf2d6b31d2fe45a2b85b8191", create: `CREATE INDEX "IDX_3caf2d6b31d2fe45a2b85b8191" ON "bid" ("nft_id")` },
  ],
  transfer: [
    { name: "IDX_024eb30e5fd99a5bea7befe60e", create: `CREATE INDEX "IDX_024eb30e5fd99a5bea7befe60e" ON "transfer" ("network")` },
    { name: "IDX_c116ab40c3b32ca2d9c1d17d8b", create: `CREATE INDEX "IDX_c116ab40c3b32ca2d9c1d17d8b" ON "transfer" ("block")` },
    { name: "IDX_be54ea276e0f665ffc38630fc0", create: `CREATE INDEX "IDX_be54ea276e0f665ffc38630fc0" ON "transfer" ("from")` },
    { name: "IDX_4cbc37e8c3b47ded161f44c24f", create: `CREATE INDEX "IDX_4cbc37e8c3b47ded161f44c24f" ON "transfer" ("to")` },
    { name: "IDX_f605a03972b4f28db27a0ee70d", create: `CREATE INDEX "IDX_f605a03972b4f28db27a0ee70d" ON "transfer" ("tx_hash")` },
  ],
  mint: [
    { name: "IDX_cd587534d4140377bb52337ae4", create: `CREATE INDEX "IDX_cd587534d4140377bb52337ae4" ON "mint" ("item_id")` },
    { name: "IDX_c46ca4e5f135d6dbdf10111660", create: `CREATE INDEX "IDX_c46ca4e5f135d6dbdf10111660" ON "mint" ("nft_id")` },
  ],
  curation: [
    { name: "IDX_dff9f3d4753a2a4caecf74d066", create: `CREATE INDEX "IDX_dff9f3d4753a2a4caecf74d066" ON "curation" ("curator_id")` },
    { name: "IDX_2cb014ad08eee6a3c64afa42f3", create: `CREATE INDEX "IDX_2cb014ad08eee6a3c64afa42f3" ON "curation" ("collection_id")` },
    { name: "IDX_ddf35815bd940a989480f79fec", create: `CREATE INDEX "IDX_ddf35815bd940a989480f79fec" ON "curation" ("item_id")` },
  ],
  metadata: [
    { name: "IDX_45072545bb44e246e0496110f9", create: `CREATE INDEX "IDX_45072545bb44e246e0496110f9" ON "metadata" ("wearable_id")` },
    { name: "IDX_cee9cecc2205cd07a21813203d", create: `CREATE INDEX "IDX_cee9cecc2205cd07a21813203d" ON "metadata" ("emote_id")` },
  ],
  wearable: [
    { name: "IDX_f011ccea27833b0628a7532834", create: `CREATE INDEX "IDX_f011ccea27833b0628a7532834" ON "wearable" ("owner_id")` },
  ],
  account: [
    { name: "IDX_83603c168bc00b20544539fbea", create: `CREATE INDEX "IDX_83603c168bc00b20544539fbea" ON "account" ("address")` },
  ],
  squid_router_order: [
    { name: "IDX_squid_router_order_order_hash", create: `CREATE INDEX "IDX_squid_router_order_order_hash" ON "squid_router_order" ("order_hash")` },
    { name: "IDX_squid_router_order_tx_hash", create: `CREATE INDEX "IDX_squid_router_order_tx_hash" ON "squid_router_order" ("tx_hash")` },
  ],
};

// Secondary indices owned exclusively by the ETH processor.
export const ETH_INDICES: IndexGroup = {
  estate: [
    { name: "IDX_1f3ec6150afbb8a3fd75fae814", create: `CREATE INDEX "IDX_1f3ec6150afbb8a3fd75fae814" ON "estate" ("size")` },
    { name: "IDX_0b680d37990796da3232ad9d98", create: `CREATE INDEX "IDX_0b680d37990796da3232ad9d98" ON "estate" ("owner_id")` },
    { name: "IDX_c40a1b5f5b764ad6ab5fa749cd", create: `CREATE INDEX "IDX_c40a1b5f5b764ad6ab5fa749cd" ON "estate" ("data_id")` },
  ],
  parcel: [
    { name: "IDX_a7c5c87cd4ffc1e1129f0c5f43", create: `CREATE INDEX "IDX_a7c5c87cd4ffc1e1129f0c5f43" ON "parcel" ("owner_id")` },
    { name: "IDX_da4912d77606dcfabe5da7eebc", create: `CREATE INDEX "IDX_da4912d77606dcfabe5da7eebc" ON "parcel" ("estate_id")` },
    { name: "IDX_04ab2b996d659d2f86dbcee860", create: `CREATE INDEX "IDX_04ab2b996d659d2f86dbcee860" ON "parcel" ("data_id")` },
  ],
  data: [
    { name: "IDX_8694618f20c7b364d4cb23c111", create: `CREATE INDEX "IDX_8694618f20c7b364d4cb23c111" ON "data" ("parcel_id")` },
    { name: "IDX_ae7e5532f8406258419ed617b4", create: `CREATE INDEX "IDX_ae7e5532f8406258419ed617b4" ON "data" ("estate_id")` },
  ],
  ens: [
    { name: "IDX_2ebf256442a48f5acbdf2ea77d", create: `CREATE INDEX "IDX_2ebf256442a48f5acbdf2ea77d" ON "ens" ("owner_id")` },
    { name: "IDX_ens_order_hash", create: `CREATE INDEX "IDX_ens_order_hash" ON "ens" ("order_hash")` },
  ],
};

export function flattenIndices(group: IndexGroup): ManagedIndex[] {
  return Object.values(group).flat();
}

// Threshold percentage for fresh sync detection (10% above initial block)
const FRESH_SYNC_THRESHOLD_PERCENT = 0.1;

// Polygon mainnet (137) => production; anything else (e.g. Amoy 80002) => dev.
const isMainnet = process.env.POLYGON_CHAIN_ID === "137";
const SQUID_ALERTS_CHANNEL = isMainnet ? "squid-alerts" : "squid-alerts-dev";

let slackComponent: ISlackComponent | undefined;
function getSlack(): ISlackComponent | undefined {
  if (slackComponent) return slackComponent;
  const botToken = process.env.SLACK_BOT_TOKEN;
  const signingSecret = process.env.SLACK_SIGNING_SECRET;
  if (!botToken || !signingSecret) return undefined;
  slackComponent = createSlackComponent({ botToken, signingSecret });
  return slackComponent;
}

// Sends a Slack alert via the shared bot-token component. Never throws -- alerting
// must not break indexing. No-op (silently) when credentials are not configured.
async function sendSlackNotification(message: string): Promise<void> {
  const slack = getSlack();
  if (!slack) return;
  try {
    await slack.sendMessage(SQUID_ALERTS_CHANNEL, message);
  } catch (e: any) {
    console.log(`${LOG_PREFIX} Slack notification error: ${e.message}`);
  }
}

function em(store: Store): EntityManager {
  return (store as unknown as { em: () => EntityManager }).em();
}

// Runs index DDL on a fresh pooled connection OUTSIDE the batch transaction, so each
// CREATE/DROP INDEX autocommits on its own instead of being held inside the
// multi-hour processor transaction. The squid's schema lives in the search_path of
// the transactional batch connection; we resolve it there and copy it onto the
// independent connection, so unqualified table/index names and current_schema()
// resolve to the squid's schema rather than public.
async function withIndexConnection<T>(
  store: Store,
  fn: (runner: QueryRunner) => Promise<T>
): Promise<T> {
  const schemaRows: { schema: string }[] = await em(store).query(
    "SELECT current_schema() AS schema"
  );
  const schema = schemaRows[0]?.schema;
  const runner = em(store).connection.createQueryRunner();
  try {
    await runner.connect();
    if (schema) {
      await runner.query(`SET search_path TO "${schema}", public`);
    }
    return await fn(runner);
  } finally {
    await runner.release();
  }
}

/**
 * Determine if this is a FRESH sync (new deploy) vs a RESTART of an already synced
 * squid. If currentBlock is within 10% of the configured initial block it is a
 * fresh sync.
 */
export function isFreshSync(currentBlock: number, initialBlock: number): boolean {
  const threshold = Math.floor(initialBlock * (1 + FRESH_SYNC_THRESHOLD_PERCENT));
  return currentBlock < threshold;
}

// Checks whether an index exists in THIS squid's schema. Filtering by
// current_schema() is essential: the production DB holds many squid schemas with
// identically-named indices, so an unscoped check would report another schema's
// index as ours and skip recreation, leaving this schema unindexed forever.
async function indexExists(runner: QueryRunner, indexName: string): Promise<boolean> {
  const result = await runner.query(
    `SELECT 1 FROM pg_indexes WHERE indexname = $1 AND schemaname = current_schema()`,
    [indexName]
  );
  return result.length > 0;
}

async function getIndicesStatus(
  runner: QueryRunner,
  indices: ManagedIndex[]
): Promise<{
  existing: string[];
  missing: string[];
  total: number;
  existingCount: number;
  missingCount: number;
}> {
  const existing: string[] = [];
  const missing: string[] = [];

  for (const idx of indices) {
    if (await indexExists(runner, idx.name)) {
      existing.push(idx.name);
    } else {
      missing.push(idx.name);
    }
  }

  return {
    existing,
    missing,
    total: indices.length,
    existingCount: existing.length,
    missingCount: missing.length,
  };
}

/**
 * Drop all managed indices in `group` for faster bulk loading. Runs each DROP on an
 * independent connection so it autocommits outside the batch transaction.
 */
export async function dropIndicesForBulkLoad(store: Store, group: IndexGroup): Promise<void> {
  // Anything the processor's own read path needs stays -- see keepDuringBulkLoad.
  const indices = flattenIndices(group).filter((idx) => !idx.keepDuringBulkLoad);
  await withIndexConnection(store, async (runner) => {
    const statusBefore = await getIndicesStatus(runner, indices);

    if (statusBefore.existingCount === 0) {
      console.log(`${LOG_PREFIX} No managed indices present. Nothing to drop.`);
      return;
    }

    console.log(
      `${LOG_PREFIX} BULK MODE: dropping ${statusBefore.existingCount} indices for faster indexing`
    );

    const startTime = performance.now();
    let dropped = 0;
    let skipped = 0;
    let failed = 0;

    for (const idx of indices) {
      try {
        if (!(await indexExists(runner, idx.name))) {
          skipped++;
          continue;
        }
        // Each DROP runs in its own implicit transaction on the independent
        // connection; a failure never aborts the others.
        await runner.query(`DROP INDEX IF EXISTS "${idx.name}"`);
        dropped++;
      } catch (e: any) {
        failed++;
        console.log(`${LOG_PREFIX} Failed to drop ${idx.name}: ${e.message}`);
      }
    }

    const durationSec = ((performance.now() - startTime) / 1000).toFixed(1);
    console.log(
      `${LOG_PREFIX} Drop complete: dropped=${dropped}, skipped=${skipped}, failed=${failed} in ${durationSec}s`
    );
  });
}

/**
 * Recreate all managed indices in `group` (call when caught up with chain head).
 * Runs each CREATE on an independent connection so it autocommits outside the batch
 * transaction.
 */
export async function recreateIndices(store: Store, group: IndexGroup): Promise<void> {
  const indices = flattenIndices(group);
  await withIndexConnection(store, async (runner) => {
    const statusBefore = await getIndicesStatus(runner, indices);

    if (statusBefore.missingCount === 0) {
      console.log(`${LOG_PREFIX} All ${statusBefore.total} indices already exist. Nothing to do.`);
      return;
    }

    console.log(
      `${LOG_PREFIX} Recreating ${statusBefore.missingCount} missing indices (may take several minutes)...`
    );
    await sendSlackNotification(
      `\u{1F527} Starting index creation: ${statusBefore.missingCount} indices to create`
    );

    const startTime = performance.now();
    let created = 0;
    let skipped = 0;
    let failed = 0;
    const failedIndices: { name: string; error: string }[] = [];

    for (const idx of indices) {
      try {
        if (await indexExists(runner, idx.name)) {
          skipped++;
          continue;
        }
        const createStatement = idx.create
          .replace("CREATE UNIQUE INDEX", "CREATE UNIQUE INDEX IF NOT EXISTS")
          .replace("CREATE INDEX", "CREATE INDEX IF NOT EXISTS");
        await runner.query(createStatement);
        created++;
      } catch (e: any) {
        if (e.message.includes("already exists")) {
          skipped++;
        } else {
          failed++;
          failedIndices.push({ name: idx.name, error: e.message });
          console.log(`${LOG_PREFIX} Failed to create ${idx.name}: ${e.message}`);
        }
      }
    }

    const durationMin = ((performance.now() - startTime) / 60000).toFixed(1);
    if (failed > 0) {
      console.log(
        `${LOG_PREFIX} Index creation completed with errors: created=${created}, skipped=${skipped}, failed=${failed}`
      );
      failedIndices.forEach(({ name, error }) =>
        console.log(`${LOG_PREFIX}    - ${name}: ${error}`)
      );
      await sendSlackNotification(
        `\u{26A0}\u{FE0F} Index creation completed with errors: ${created} created, ${failed} failed. Duration: ${durationMin} min.`
      );
    } else {
      console.log(
        `${LOG_PREFIX} Index creation complete: created=${created}, skipped=${skipped} in ${durationMin} min`
      );
      await sendSlackNotification(
        `\u{2705} All indices created successfully! ${created} indices created in ${durationMin} minutes.`
      );
    }

    // Throw rather than return: per-index failures are caught inside the loop above, so without
    // this the caller sees a clean return, latches "indices recreated" and never retries -- leaving
    // production permanently missing an index with nothing but a Slack message to say so.
    const statusAfter = await getIndicesStatus(runner, indices);
    if (statusAfter.missingCount > 0) {
      throw new Error(
        `${statusAfter.missingCount} of ${statusAfter.total} indices are still missing after the recreate pass`
      );
    }
  });
}

export async function checkIndicesNeedRecreation(store: Store, group: IndexGroup): Promise<boolean> {
  const indices = flattenIndices(group);
  const status = await withIndexConnection(store, (runner) =>
    getIndicesStatus(runner, indices)
  );
  const needsRecreation = status.missingCount > 0;

  if (needsRecreation) {
    console.log(
      `${LOG_PREFIX} Startup check: ${status.missingCount}/${status.total} indices missing, will recreate at head`
    );
  } else {
    console.log(`${LOG_PREFIX} Startup check: all ${status.total} indices exist`);
  }
  return needsRecreation;
}

export function logIndexConfiguration(group: IndexGroup, initialBlock?: number): void {
  const indices = flattenIndices(group);
  const byTable = Object.entries(group)
    .map(([table, idx]) => `${table}=${idx.length}`)
    .join(", ");
  console.log(`${LOG_PREFIX} Managing ${indices.length} indices (${byTable})`);
  if (initialBlock) {
    const threshold = Math.floor(initialBlock * (1 + FRESH_SYNC_THRESHOLD_PERCENT));
    console.log(
      `${LOG_PREFIX} Fresh-sync threshold: block < ${threshold.toLocaleString()} (initial ${initialBlock.toLocaleString()} + ${FRESH_SYNC_THRESHOLD_PERCENT * 100}%)`
    );
  }
}
