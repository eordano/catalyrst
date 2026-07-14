import { z } from "zod";

import fixture from "../../../fixtures/admin-debug-console.json";
import { getJSON, catalystBase, CatalystError } from "../client";
import type { GetOptions } from "../client";
import {
  BudgetRowSchema,
  BudgetsEnvelopeSchema,
} from "../generated-schemas/governance";
import { controlStatus } from "../admin/control-availability";
import type { Unavailable } from "../admin/availability";

const HealthSchema = z.object({
  service: z.string(),
  status: z.string(),
  ok: z.boolean(),
  checkedPath: z.string(),
});

const EnvVarSchema = z.object({
  name: z.string(),
  value: z.string(),
});

const SnapshotSchema = z.object({
  space: z.string(),
  network: z.string(),
  config: z.string(),
  spaceInfo: z.string(),
});

const FixtureSchema = z.object({
  version: z.string(),
  health: HealthSchema,
  snapshot: SnapshotSchema,
  env: z.array(EnvVarSchema),
  functions: z.array(z.string()),
});

export type BudgetRow = z.infer<typeof BudgetRowSchema>;
export type HealthInfo = z.infer<typeof HealthSchema>;
export type EnvVar = z.infer<typeof EnvVarSchema>;
export type SnapshotInfo = z.infer<typeof SnapshotSchema>;

export type BudgetSummary = {
  start: string;
  finish: string;
  total: number;
  categories: { name: string; pct: number }[];
};

/**
 * Both reads behind this page are PUBLIC. Read this session:
 *   catalyrst-governance/src/handlers/health.rs:3  `health()` -- no extractor
 *   catalyrst-governance/src/handlers/read.rs:220  `budgets()` -- no extractor
 *
 * Neither takes an auth extractor of any kind, so the page renders them
 * unconditionally and must label them "public data -- no authorization
 * required". There is no privileged read here to gate.
 *
 * The "tools" section always renders a permanent unavailable state
 * (`control-availability.ts` -> `debug.tools`): no privileged tooling is
 * wired on this node, and nothing here makes an HTTP call for it.
 */
export type DebugConsoleData = {
  /**
   * Per-section provenance. `health` and `budgets` are independent: the node
   * can be up while holding no budget rows.
   */
  provenance: "public";
  version: string;
  /** null when the health probe could not be completed. Never a fixture. */
  health: HealthInfo | null;
  healthReason: string | null;
  env: EnvVar[];
  snapshot: SnapshotInfo;
  /** null when budgets could not be read. Never a fixture. */
  budgets: BudgetSummary[] | null;
  budgetsReason: string | null;
  /** Always unavailable. See the comment above. */
  tools: Unavailable;
};

function toSummary(row: BudgetRow): BudgetSummary {
  const categories = Object.entries(row.categories)
    .map(([name, cat]) => ({
      name,
      pct: row.total > 0 ? Math.round((cat.total / row.total) * 1000) / 10 : 0,
    }))
    .sort((a, b) => b.pct - a.pct);
  return {
    start: row.start_at,
    finish: row.finish_at,
    total: row.total,
    categories,
  };
}

/**
 * Static page chrome only: build version, the env-var *names* the page
 * documents, and the Snapshot space identifiers. These are descriptive labels,
 * not measurements -- no count, status or budget figure is taken from here any
 * more.
 */
function staticChrome(): {
  version: string;
  env: EnvVar[];
  snapshot: SnapshotInfo;
} {
  const parsed = FixtureSchema.safeParse(fixture);
  if (!parsed.success) {
    return {
      version: "v0.0.0",
      env: [],
      snapshot: {
        space: "snapshot.dcl.eth",
        network: "1",
        config: "{}",
        spaceInfo: "{}",
      },
    };
  }
  return {
    version: parsed.data.version,
    env: parsed.data.env,
    snapshot: parsed.data.snapshot,
  };
}

type Probe<T> = { value: T; reason: null } | { value: null; reason: string };

async function probeHealth(opts: GetOptions): Promise<Probe<HealthInfo>> {
  const base = (opts.base ?? catalystBase()).replace(/\/$/, "");
  const fetchImpl = opts.fetchImpl ?? fetch;
  const url = `${base}/governance-api/health`;
  try {
    const res = await fetchImpl(url, {
      signal: opts.signal,
      headers: { accept: "application/json" },
    });
    return {
      value: {
        service: "catalyrst-governance",
        status: res.ok ? "ok" : `http_${res.status}`,
        ok: res.ok,
        checkedPath: "/governance-api/health",
      },
      reason: null,
    };
  } catch (err) {
    return {
      value: null,
      reason: `/governance-api/health unreachable: ${
        err instanceof Error ? err.message : String(err)
      }`,
    };
  }
}

async function fetchBudgets(
  opts: GetOptions,
  limit = 8,
): Promise<Probe<BudgetSummary[]>> {
  let raw: unknown;
  try {
    raw = await getJSON<unknown>("/governance-api/budgets", {
      ...opts,
      query: { limit, ...(opts.query ?? {}) },
    });
  } catch (err) {
    const status = err instanceof CatalystError ? err.status : 0;
    return {
      value: null,
      reason: status
        ? `/governance-api/budgets returned HTTP ${status}`
        : `/governance-api/budgets unreachable: ${
            err instanceof Error ? err.message : String(err)
          }`,
    };
  }
  const parsed = BudgetsEnvelopeSchema.safeParse(raw);
  if (!parsed.success) {
    return {
      value: null,
      reason: "/governance-api/budgets returned an unrecognised payload",
    };
  }
  if (parsed.data.data.length === 0) {
    return { value: null, reason: "this node holds no budget periods" };
  }
  return { value: parsed.data.data.map(toSummary), reason: null };
}

/**
 * Reads two public endpoints and reports exactly what they said. A failed
 * probe surfaces via its `reason` field and is never replaced with fixture
 * data -- a dead governance service must not render as a healthy status with
 * a full budget table.
 */
export async function loadDebugConsole(
  opts: GetOptions = {},
): Promise<DebugConsoleData> {
  const chrome = staticChrome();

  const [health, budgets] = await Promise.all([
    probeHealth(opts),
    fetchBudgets(opts),
  ]);

  return {
    provenance: "public",
    ...chrome,
    health: health.value,
    healthReason: health.reason,
    budgets: budgets.value,
    budgetsReason: budgets.reason,
    tools: controlStatus("debug.tools") as Unavailable,
  };
}
