import type { Pool } from "pg";

import { LLM_REFERENCE_PRICING } from "./types";
import type { LlmUsageDay, LlmUsageRow, LlmUsageSummary } from "./types";

// Token counts come from the gateway's own per-message accounting, stored
// verbatim. The dollar column is arithmetic on those counts at a price we chose,
// and every surface that shows it also shows LLM_REFERENCE_PRICING.label.
//
// `cost_usd` is the figure the copilot itself computed at ingest time; when it
// is absent the sum falls back to the row's own recorded prices rather than
// today's constant, so a later price change cannot rewrite what a past month
// cost.

const COST_EXPR = `coalesce(cost_usd,
    input_tokens / 1e6 * price_input_per_m + output_tokens / 1e6 * price_output_per_m)`;

export interface LlmUsageInput {
  messageId: string;
  sessionId: string;
  sessionTitle: string | null;
  model: string;
  inputTokens: number;
  outputTokens: number;
  reasoningTokens?: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
  costUsd: number | null;
  at: string;
}

/** Keyed by the gateway's own message id, so re-reading a session is an update. */
export async function upsertLlmUsage(db: Pool, row: LlmUsageInput): Promise<void> {
  await db.query(
    `INSERT INTO foundry.llm_usage
       (message_id, session_id, session_title, model, input_tokens, output_tokens,
        reasoning_tokens, cache_read_tokens, cache_write_tokens, cost_usd,
        price_input_per_m, price_output_per_m, at)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13::timestamptz)
     ON CONFLICT (message_id) DO UPDATE SET
       session_id         = EXCLUDED.session_id,
       session_title      = EXCLUDED.session_title,
       model              = EXCLUDED.model,
       input_tokens       = EXCLUDED.input_tokens,
       output_tokens      = EXCLUDED.output_tokens,
       reasoning_tokens   = EXCLUDED.reasoning_tokens,
       cache_read_tokens  = EXCLUDED.cache_read_tokens,
       cache_write_tokens = EXCLUDED.cache_write_tokens,
       cost_usd           = EXCLUDED.cost_usd,
       at                 = EXCLUDED.at,
       ingested_at        = now()`,
    [
      row.messageId,
      row.sessionId,
      row.sessionTitle,
      row.model,
      row.inputTokens,
      row.outputTokens,
      row.reasoningTokens ?? 0,
      row.cacheReadTokens ?? 0,
      row.cacheWriteTokens ?? 0,
      row.costUsd,
      LLM_REFERENCE_PRICING.inputPerM,
      LLM_REFERENCE_PRICING.outputPerM,
      row.at,
    ],
  );
}

type TotalsDbRow = {
  messages: number;
  sessions: number;
  input_tokens: string;
  output_tokens: string;
  cost_usd: number;
};

type DayDbRow = {
  day: string;
  input_tokens: string;
  output_tokens: string;
  cost_usd: number;
};

type RecentDbRow = {
  message_id: string;
  session_id: string;
  session_title: string | null;
  model: string;
  input_tokens: number;
  output_tokens: number;
  reasoning_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  cost_usd: string | null;
  price_input_per_m: string;
  price_output_per_m: string;
  at: Date | string;
};

const EMPTY: LlmUsageSummary = {
  messages: 0,
  sessions: 0,
  inputTokens: 0,
  outputTokens: 0,
  costUsd: 0,
  byDay: [],
  recent: [],
};

function toRow(r: RecentDbRow): LlmUsageRow {
  return {
    messageId: r.message_id,
    sessionId: r.session_id,
    sessionTitle: r.session_title,
    model: r.model,
    inputTokens: Number(r.input_tokens),
    outputTokens: Number(r.output_tokens),
    reasoningTokens: Number(r.reasoning_tokens),
    cacheReadTokens: Number(r.cache_read_tokens),
    cacheWriteTokens: Number(r.cache_write_tokens),
    costUsd: r.cost_usd === null ? null : Number(r.cost_usd),
    priceInputPerM: Number(r.price_input_per_m),
    priceOutputPerM: Number(r.price_output_per_m),
    at: r.at instanceof Date ? r.at.toISOString() : String(r.at),
  };
}

export async function llmUsageSummary(
  db: Pool,
  opts: { recent?: number } = {},
): Promise<LlmUsageSummary> {
  const limit = opts.recent ?? 10;

  const [totals, days, recent] = await Promise.all([
    db.query<TotalsDbRow>(
      `SELECT count(*)::int AS messages,
              count(DISTINCT session_id)::int AS sessions,
              coalesce(sum(input_tokens), 0)::bigint AS input_tokens,
              coalesce(sum(output_tokens), 0)::bigint AS output_tokens,
              coalesce(sum(${COST_EXPR}), 0)::float8 AS cost_usd
         FROM foundry.llm_usage`,
    ),
    db.query<DayDbRow>(
      `SELECT to_char(at AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS day,
              sum(input_tokens)::bigint AS input_tokens,
              sum(output_tokens)::bigint AS output_tokens,
              sum(${COST_EXPR})::float8 AS cost_usd
         FROM foundry.llm_usage
        GROUP BY 1
        ORDER BY 1 DESC
        LIMIT 60`,
    ),
    db.query<RecentDbRow>(
      `SELECT message_id, session_id, session_title, model, input_tokens,
              output_tokens, reasoning_tokens, cache_read_tokens,
              cache_write_tokens, cost_usd, price_input_per_m,
              price_output_per_m, at
         FROM foundry.llm_usage
        ORDER BY at DESC, message_id
        LIMIT $1`,
      [limit],
    ),
  ]);

  const t = totals.rows[0];
  if (!t) return EMPTY;

  const byDay: LlmUsageDay[] = days.rows.map((d) => ({
    day: d.day,
    inputTokens: Number(d.input_tokens),
    outputTokens: Number(d.output_tokens),
    costUsd: Number(d.cost_usd),
  }));

  return {
    messages: Number(t.messages),
    sessions: Number(t.sessions),
    inputTokens: Number(t.input_tokens),
    outputTokens: Number(t.output_tokens),
    costUsd: Number(t.cost_usd),
    byDay,
    recent: recent.rows.map(toRow),
  };
}
