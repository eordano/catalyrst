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
  source: "live" | "fixture" | "unavailable";
  reason?: string;
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
