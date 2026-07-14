import { Store } from "@subsquid/typeorm-store";
import { EntityManager, QueryRunner } from "typeorm";
import { createSlackComponent, ISlackComponent } from "./slack";
import { deploymentSchema } from "./deployment-schema";

// Polygon mainnet (137) => production; anything else (e.g. Amoy 80002) => zone/dev.
const isMainnet = process.env.POLYGON_CHAIN_ID === "137";
const SQUID_ALERTS_CHANNEL = isMainnet ? "squid-alerts" : "squid-alerts-dev";
const ENV_LABEL = isMainnet ? "prd" : "dev";

// DELIBERATE DIVERGENCE from upstream 1a65069, which references this table
// UNQUALIFIED so it follows the promotion rename (marketplace_squid_<ts> ->
// squid_marketplace). We have no promotion path -- upgrade.sh is never invoked here,
// the deployment creates squid_marketplace directly -- so the rename that motivates the
// unqualified name cannot happen, and qualifying keeps the status table unique per
// deployment and isolated from other indexers, independent of whatever search_path
// the connection below ends up with. Keep it qualified; do not "fix" it to match
// upstream without first wiring a promotion path.
// Falls back to an unqualified name in local dev where no schema var is set.
const SCHEMA = deploymentSchema();
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

// These helpers run once per batch until they succeed. If the DB rejects them
// persistently (e.g. a schema/permission problem), retrying forever would print a full
// stack trace per batch and pay a connection checkout per batch, for the whole life of
// the process -- so give up after a few attempts; alerting is best-effort.
const MAX_ATTEMPTS = 5;
const startAttempts = new Map<string, number>();
const headAttempts = new Map<string, number>();

function shouldAttempt(
  attempts: Map<string, number>,
  chain: string,
  op: string
): boolean {
  const n = (attempts.get(chain) ?? 0) + 1;
  attempts.set(chain, n);
  if (n === MAX_ATTEMPTS + 1) {
    console.log(
      `[SLACK] Giving up on ${op} after ${MAX_ATTEMPTS} failed attempts for ${chain}`
    );
  }
  return n <= MAX_ATTEMPTS;
}

// Records when this indexer began syncing a chain. Idempotent: the first call (this
// indexer, ever) sets started_at; later calls/restarts are no-ops, so duration is
// measured from the original start even across restarts.
export async function recordIndexingStart(store: Store, chain: string): Promise<void> {
  if (startedThisProcess.has(chain)) return;
  if (!shouldAttempt(startAttempts, chain, "indexing-start recording")) return;
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
// atomic upsert `... DO UPDATE ... WHERE head_reached_at IS NULL RETURNING`, run on an
// independent (autocommitting) connection, is the exactly-once gate per
// indexer/chain -- durable across restarts and immune to batch retries/rollbacks.
// Never throws -- alerting must not break indexing.
export async function notifyHeadReachedOnce(
  store: Store,
  chain: string,
  headBlock: number
): Promise<void> {
  if (headHandledThisProcess.has(chain)) return;
  if (!shouldAttempt(headAttempts, chain, "head-reached alert")) return;
  try {
    // Upsert instead of a plain UPDATE so the alert still fires when the
    // start-recording row is missing (recordIndexingStart exhausted its attempts);
    // duration then degrades to ~0s, which beats losing the alert. The WHERE on the
    // conflict arm keeps the exactly-once gate: an already-announced row updates
    // nothing. Aliased so the conflict arm can name the row without repeating the
    // schema-qualified table name.
    const rows: { started_at: string; head_reached_at: string }[] =
      await withOwnConnection(store, async (runner) => {
        await ensureTable(runner);
        return runner.query(
          `INSERT INTO ${STATUS_TABLE} AS s (chain, head_reached_at)
           VALUES ($1, now())
           ON CONFLICT (chain) DO UPDATE SET head_reached_at = now()
           WHERE s.head_reached_at IS NULL
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
      `:white_check_mark: *marketplace-squid [${chain}]* reached head \u{2014} initial indexing complete`,
      `\u{2022} *Env:* \`${ENV_LABEL}\``,
      `\u{2022} *Head block:* \`${headBlock}\``,
      `\u{2022} *Indexing time:* \`${formatDuration(durationMs)}\``,
      `\u{2022} *Started:* \`${new Date(started_at).toISOString()}\``,
      `\u{2022} *Finished:* \`${new Date(head_reached_at).toISOString()}\``,
      `\u{2022} *Indexer:* \`${SCHEMA ?? "unknown"}\``,
    ].join("\n");

    const result = await slack.sendMessage(SQUID_ALERTS_CHANNEL, message);
    console.log(`[SLACK] head-reached alert for ${chain}: ok=${result.ok}`);
  } catch (e) {
    console.log(`[SLACK] Failed to send head-reached alert for ${chain}:`, e);
  }
}
