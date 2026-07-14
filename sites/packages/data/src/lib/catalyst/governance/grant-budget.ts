import { z } from "zod";

import fixture from "../../../fixtures/governance-submit-grant.json";
import { warnInvalid } from "../warn";
import { BudgetRowSchema } from "../generated-schemas/governance";
import { governanceApiBase } from "./api-base";

export { governanceApiBase };

export type GrantCategory = {
  key: string;
  id: string;
  tone: string;
  desc: string;
  total: number;
  allocated: number;
  available: number;
  totalLabel: string;
  availableLabel: string;
  availablePct: number;
  suspended: boolean;
};

export type GrantTier = {
  id: string;
  min: number;
  max: number;
  passThreshold: string;
  payout: string;
  vesting: string;
};

export type GrantPeriod = {
  id: string;
  startAt: string;
  finishAt: string;
  label: string;
  total: number;
  allocated: number;
  totalLabel: string;
};

export type GrantBudget = {
  /**
   * "live" -- every number below came from GET /budgets on this node.
   * "unavailable" -- no number below is a measurement; `reason` says why and
   *   `categories` is empty. Never render `unavailable` as zeros: a zero reads
   *   as "the budget is spent", which is a different claim from "we do not
   *   know". Callers must show the reason instead.
   * "fixture" -- only produced by `fixtureBudget()`, which exists for stories
   *   and layout work. `loadGrantBudget` never returns it.
   */
  source: "live" | "fixture" | "unavailable";
  /** Set when source !== "live". Safe to show to a visitor. */
  reason?: string;
  /**
   * finish_at of the newest budget period the node holds. The mirror is only
   * as fresh as the last sync (GOVERNANCE_POLL_ENABLED is unset, so the sync
   * loop never runs -- catalyrst-governance/src/config.rs:133), so this is the
   * honest "data as of" for the page.
   */
  asOf?: string;
  submissionThresholdVp: string;
  period: GrantPeriod;
  categories: GrantCategory[];
  tiers: GrantTier[];
};

type Fixture = {
  submissionThresholdVp: string;
  period: GrantPeriod;
  categories: GrantCategory[];
  tiers: GrantTier[];
};

const FIXTURE = fixture as unknown as Fixture;

export function fixtureBudget(): GrantBudget {
  return {
    source: "fixture",
    submissionThresholdVp: FIXTURE.submissionThresholdVp,
    period: FIXTURE.period,
    categories: FIXTURE.categories,
    tiers: FIXTURE.tiers,
  };
}

const CATEGORY_META: Record<
  string,
  { id: string; tone: string; desc: string }
> = Object.fromEntries(
  FIXTURE.categories.map((c) => [c.key, { id: c.id, tone: c.tone, desc: c.desc }]),
);

const CATEGORY_ORDER = FIXTURE.categories.map((c) => c.key);

const BudgetPeriodSchema = BudgetRowSchema;

const BudgetAllSchema = z.object({
  ok: z.boolean().optional(),
  data: z.array(BudgetPeriodSchema),
});

type BudgetPeriod = z.infer<typeof BudgetPeriodSchema>;

function money(n: number): string {
  return "$" + Math.round(n).toLocaleString("en-US");
}

function quarterLabel(startIso: string): string {
  const d = new Date(startIso);
  if (Number.isNaN(d.getTime())) return "";
  const q = Math.floor(d.getUTCMonth() / 3) + 1;
  return `Q${q} ${d.getUTCFullYear()}`;
}

function projectBudget(latest: BudgetPeriod): GrantBudget {
  const categories: GrantCategory[] = CATEGORY_ORDER.map((key) => {
    const meta = CATEGORY_META[key];
    const c = latest.categories[key] ?? { total: 0, allocated: 0, available: 0 };
    const pct = c.total > 0 ? Math.round((c.available / c.total) * 100) : 0;
    return {
      key,
      id: meta?.id ?? key,
      tone: meta?.tone ?? "neutral",
      desc: meta?.desc ?? "",
      total: c.total,
      allocated: c.allocated,
      available: c.available,
      totalLabel: money(c.total),
      availableLabel: money(c.available),
      availablePct: pct,
      suspended: c.total <= 0,
    };
  });

  return {
    source: "live",
    asOf: latest.finish_at,
    submissionThresholdVp: FIXTURE.submissionThresholdVp,
    period: {
      id: latest.id,
      startAt: latest.start_at,
      finishAt: latest.finish_at,
      label: quarterLabel(latest.start_at),
      total: latest.total,
      allocated: latest.allocated,
      totalLabel: money(latest.total),
    },
    categories,
    tiers: FIXTURE.tiers,
  };
}

const EMPTY_PERIOD: GrantPeriod = {
  id: "",
  startAt: "",
  finishAt: "",
  label: "",
  total: 0,
  allocated: 0,
  totalLabel: "",
};

export function unavailableBudget(reason: string): GrantBudget {
  return {
    source: "unavailable",
    reason,
    submissionThresholdVp: FIXTURE.submissionThresholdVp,
    period: EMPTY_PERIOD,
    categories: [],
    tiers: FIXTURE.tiers,
  };
}

export type LoadBudgetOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
};

/**
 * GET /budgets on catalyrst-governance (crates/catalyrst-governance/src/lib.rs:41,
 * handler handlers/read.rs:220 -- a public read, no auth extractor of any kind).
 * A failure is reported via `unavailableBudget`, never replaced with
 * `fixtureBudget()`'s plausible-looking money.
 */
export async function loadGrantBudget(
  opts: LoadBudgetOptions = {},
): Promise<GrantBudget> {
  const base = governanceApiBase(opts.base);
  const url = `${base}/budgets`;
  const doFetch = opts.fetchImpl ?? fetch;

  try {
    const res = await doFetch(url, {
      headers: { accept: "application/json" },
      signal: opts.signal,
    });
    if (!res.ok) {
      return unavailableBudget(
        `governance budgets endpoint returned ${res.status}`,
      );
    }
    const raw = (await res.json()) as unknown;
    const parsed = BudgetAllSchema.safeParse(raw);
    if (!parsed.success) {
      warnInvalid("governance /budgets", parsed.error?.issues);
      return unavailableBudget(
        "governance budgets endpoint returned an unrecognised payload",
      );
    }
    if (parsed.data.data.length === 0) {
      return unavailableBudget("this node holds no budget periods");
    }

    const latest = parsed.data.data.reduce((a, b) =>
      a.start_at >= b.start_at ? a : b,
    );
    return projectBudget(latest);
  } catch (err) {
    return unavailableBudget(
      `governance budgets endpoint unreachable: ${
        err instanceof Error ? err.message : String(err)
      }`,
    );
  }
}
