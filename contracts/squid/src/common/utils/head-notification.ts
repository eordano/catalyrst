import { Store } from "@subsquid/typeorm-store";
import { EntityManager, QueryRunner } from "typeorm";
import { createSlackComponent, ISlackComponent } from "./slack";

// Polygon mainnet (137) => production; anything else (e.g. Amoy 80002) => zone/dev.
const isMainnet = process.env.POLYGON_CHAIN_ID === "137";
const SQUID_ALERTS_CHANNEL = isMainnet ? "squid-alerts" : "squid-alerts-dev";
const ENV_LABEL = isMainnet ? "prd" : "dev";

// Fully-qualify the status table with the indexer's own schema, so it is unique per
// deployment and isolated from other indexers — independent of the connection's
// search_path (the independent connection below may not inherit the indexer schema).
// Falls back to an unqualified name in local dev where DB_SCHEMA is unset.
const SCHEMA = process.env.DB_SCHEMA;
const STATUS_TABLE = SCHEMA ? `"${SCHEMA}".head_sync_status` : "head_sync_status";

let slackComponent: ISlackComponent | undefined;
function getSlack(): ISlackComponent | undefined {
  if (slackComponent) return slackComponent;
  const botToken = process.env.SLACK_BOT_TOKEN;
  const signingSecret = process.env.SLACK_SIGNING_SECRET;
  if (!botToken || !signingSecret) return undefined;
  slackComponent = createSlackComponent({ botToken, signingSecret });
  return slackComponent;
}

function em(store: Store): EntityManager {
  return (store as unknown as { em: () => EntityManager }).em();
}

// Runs `fn` on a fresh pooled connection that is NOT part of the batch transaction,
// so its writes autocommit immediately and are never rolled back or replayed when
// subsquid retries a batch on a serialization conflict. This is what makes the
// "alert exactly once per indexer" gate durable (the batch `store.em()` is
// transactional and therefore unsafe for a one-shot side effect like a Slack post).
async function withOwnConnection<T>(
  store: Store,
  fn: (runner: QueryRunner) => Promise<T>
): Promise<T> {
  const runner = em(store).connection.createQueryRunner();
  try {
    await runner.connect();
    return await fn(runner);
  } finally {
    await runner.release();
  }
}

let tableEnsured = false;
async function ensureTable(runner: QueryRunner): Promise<void> {
  if (tableEnsured) return;
  await runner.query(
    `CREATE TABLE IF NOT EXISTS ${STATUS_TABLE} (
       chain text PRIMARY KEY,
       started_at timestamptz NOT NULL DEFAULT now(),
       head_reached_at timestamptz
     )`
  );
  tableEnsured = true;
}

const startedThisProcess = new Set<string>();
const headHandledThisProcess = new Set<string>();

// Records when this indexer began syncing a chain. Idempotent: the first call (this
// indexer, ever) sets started_at; later calls/restarts are no-ops, so duration is
// measured from the original start even across restarts.
export async function recordIndexingStart(store: Store, chain: string): Promise<void> {
  if (startedThisProcess.has(chain)) return;
  try {
    await withOwnConnection(store, async (runner) => {
      await ensureTable(runner);
      await runner.query(
        `INSERT INTO ${STATUS_TABLE} (chain) VALUES ($1) ON CONFLICT (chain) DO NOTHING`,
        [chain]
      );
    });
    startedThisProcess.add(chain);
  } catch (e) {
    console.log(`[SLACK] Failed to record indexing start for ${chain}:`, e);
  }
}

function formatDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = totalSeconds % 60;
  const parts: string[] = [];
  if (h) parts.push(`${h}h`);
  if (h || m) parts.push(`${m}m`);
  parts.push(`${s}s`);
  return parts.join(" ");
}

// Posts a Slack alert the FIRST time this indexer reaches head for `chain`. The
// atomic `UPDATE ... WHERE head_reached_at IS NULL RETURNING`, run on an
// independent (autocommitting) connection, is the exactly-once gate per
// indexer/chain — durable across restarts and immune to batch retries/rollbacks.
// Never throws — alerting must not break indexing.
export async function notifyHeadReachedOnce(
  store: Store,
  chain: string,
  headBlock: number
): Promise<void> {
  if (headHandledThisProcess.has(chain)) return;
  try {
    const rows: { started_at: string; head_reached_at: string }[] =
      await withOwnConnection(store, async (runner) => {
        await ensureTable(runner);
        return runner.query(
          `UPDATE ${STATUS_TABLE}
             SET head_reached_at = now()
           WHERE chain = $1 AND head_reached_at IS NULL
           RETURNING started_at, head_reached_at`,
          [chain]
        );
      });
    // The gate is committed independently above. Mark handled so we stop querying
    // every batch in this process; the committed row is the cross-restart guard.
    headHandledThisProcess.add(chain);
    if (!rows || rows.length === 0) return; // already announced for this indexer/chain

    const { started_at, head_reached_at } = rows[0];
    const durationMs =
      new Date(head_reached_at).getTime() - new Date(started_at).getTime();

    const slack = getSlack();
    if (!slack) {
      console.log(`[SLACK] Credentials not set, skipping head-reached alert for ${chain}`);
      return;
    }

    const message = [
      `:white_check_mark: *marketplace-squid [${chain}]* reached head — initial indexing complete`,
      `• *Env:* \`${ENV_LABEL}\``,
      `• *Head block:* \`${headBlock}\``,
      `• *Indexing time:* \`${formatDuration(durationMs)}\``,
      `• *Started:* \`${new Date(started_at).toISOString()}\``,
      `• *Finished:* \`${new Date(head_reached_at).toISOString()}\``,
      `• *Indexer:* \`${SCHEMA ?? "unknown"}\``,
    ].join("\n");

    const result = await slack.sendMessage(SQUID_ALERTS_CHANNEL, message);
    console.log(`[SLACK] head-reached alert for ${chain}: ok=${result.ok}`);
  } catch (e) {
    console.log(`[SLACK] Failed to send head-reached alert for ${chain}:`, e);
  }
}
